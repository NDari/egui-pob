-- Compare tab helpers: data-shaped wrappers over upstream CompareTab /
-- CompareEntry. pob_compare.statRows is a port of CompareTab:DrawStatList
-- plus DrawSummary's actor/output selection and the file-local matchFlags,
-- emitting rows instead of DrawString calls (registered in ports.toml).

local function matchFlags(reqFlags, notFlags, flags)
    if type(reqFlags) == "string" then
        reqFlags = { reqFlags }
    end
    if reqFlags then
        for _, flag in ipairs(reqFlags) do
            if not flags[flag] then
                return
            end
        end
    end
    if type(notFlags) == "string" then
        notFlags = { notFlags }
    end
    if notFlags then
        for _, flag in ipairs(notFlags) do
            if flags[flag] then
                return
            end
        end
    end
    return true
end

pob_compare = {}

-- Upstream bug fix (see DIVERGENCES.md): a CompareEntry never reprocesses its
-- socket groups, so its gems never pick up the matching-socket quality bonus
-- and every quality-derived stat reads low against the primary build.
--
-- Build.lua does this before every BuildOutput:
--     -- reprocess socket groups as they might depend on items which don't
--     -- necessarily load first.
--     self.skillsTab:UpdateSocketGroups()
-- but CompareEntry:LoadFromXML and CompareEntry:Rebuild both skip it, and
-- CompareEntry loads its Skills section before its Items section, so the
-- socket colours are simply not known yet when the gems are processed.
--
-- Decorating the class rather than porting the loader: the fix is one call in
-- the right place, and wrapping keeps the rest of upstream's loading intact.
--
-- The class is registered lazily, on the first import, so this cannot run at
-- load time - it is applied on demand and is idempotent.
local function patchCompareEntryRebuild()
    local entryClass = common.classes and common.classes["CompareEntry"]
    if not entryClass or entryClass.pobEguiSocketGroupFix then
        return
    end
    entryClass.pobEguiSocketGroupFix = true

    local innerRebuild = entryClass.Rebuild
    entryClass.Rebuild = function(self, ...)
        if self.skillsTab and self.skillsTab.UpdateSocketGroups then
            self.skillsTab:UpdateSocketGroups()
        end
        return innerRebuild(self, ...)
    end
end

-- Re-run an entry's output now that its items are loaded. Needed after a fresh
-- import: LoadFromXML does its own BuildOutput before the patch above can help
-- it, since the class does not exist until that first load is under way.
local function rebuildEntrySocketGroups(entry)
    patchCompareEntryRebuild()
    if entry and entry.Rebuild then
        entry:Rebuild()
    end
end

local function getBuild()
    return mainObject_ref.main.modes['BUILD']
end

function pob_compare.import(xml, label)
    local tab = getBuild().compareTab
    local ok = tab:ImportBuild(xml, label)
    if ok then
        rebuildEntrySocketGroups(tab.compareEntries[#tab.compareEntries])
    end
    return ok and true or false
end

function pob_compare.importCode(code)
    local tab = getBuild().compareTab
    local ok = tab:ImportFromCode(code)
    if ok then
        rebuildEntrySocketGroups(tab.compareEntries[#tab.compareEntries])
    end
    return ok and true or false
end

function pob_compare.list()
    local tab = getBuild().compareTab
    local out = { active = tab.activeCompareIndex or 0, entries = {} }
    for i, entry in ipairs(tab.compareEntries) do
        table.insert(out.entries, entry.label or ("Build " .. i))
    end
    return out
end

function pob_compare.remove(i)
    getBuild().compareTab:RemoveBuild(i)
end

function pob_compare.setActive(i)
    local tab = getBuild().compareTab
    if tab.compareEntries[i] then
        tab.activeCompareIndex = i
    end
end

-- Primary build calc revision, for cheap staleness checks in the GUI.
function pob_compare.revision()
    return getBuild().outputRevision or 0
end

local function getEntry(index)
    return getBuild().compareTab.compareEntries[index]
end

-- The compare entry's active spec allocation (for the Rust-side tree diff).
function pob_compare.specAllocation(index)
    local entry = getEntry(index)
    local out = { nodes = {}, masteries = {} }
    if not entry or not entry.spec then
        return out
    end
    for nodeId in pairs(entry.spec.allocNodes or {}) do
        if type(nodeId) == "number" then
            table.insert(out.nodes, nodeId)
        end
    end
    for nodeId, effectId in pairs(entry.spec.masterySelections or {}) do
        table.insert(out.masteries, { node = nodeId, effect = effectId })
    end
    out.treeVersion = entry.spec.treeVersion
    return out
end

-- Named node diff for the tree view (nodes allocated on one side only, and
-- masteries with differing effects), resolved against the primary tree.
function pob_compare.treeDiff(index)
    local build = getBuild()
    local entry = getEntry(index)
    local out = { added = {}, removed = {}, mastery = {}, version = "" }
    if not entry or not entry.spec or not build.spec then
        return out
    end
    out.version = entry.spec.treeVersion or ""
    local pNodes = build.spec.allocNodes or {}
    local cNodes = entry.spec.allocNodes or {}
    local function nodeName(spec, nodeId)
        local node = spec.nodes and spec.nodes[nodeId]
        return node and (node.dn or node.name) or ("Node " .. nodeId)
    end
    for nodeId in pairs(cNodes) do
        if type(nodeId) == "number" and nodeId < 65536 and not pNodes[nodeId] then
            table.insert(out.added, nodeName(entry.spec, nodeId))
        end
    end
    for nodeId in pairs(pNodes) do
        if type(nodeId) == "number" and nodeId < 65536 and not cNodes[nodeId] then
            table.insert(out.removed, nodeName(build.spec, nodeId))
        end
    end
    for nodeId, effectId in pairs(build.spec.masterySelections or {}) do
        local other = entry.spec.masterySelections and entry.spec.masterySelections[nodeId]
        if other and other ~= effectId then
            table.insert(out.mastery, nodeName(build.spec, nodeId))
        end
    end
    table.sort(out.added)
    table.sort(out.removed)
    table.sort(out.mastery)
    return out
end

function pob_compare.copySpec(andUse)
    getBuild().compareTab:CopyCompareSpecToPrimary(andUse and true or false)
end

-- Item slot comparison rows: upstream DrawItems' slot list (base slots +
-- Ring 3 + abyss sockets, requireBothSides=false) plus the jewel union from
-- GetJewelComparisonSlots, with tradeHelpers.getSlotDiffLabel statuses.
function pob_compare.itemRows(index)
    local build = getBuild()
    local tab = build.compareTab
    local entry = getEntry(index)
    local out = {}
    if not entry then
        return out
    end
    local tradeHelpers = LoadModule("Classes/TradeHelpers")
    local baseSlots = { "Weapon 1", "Weapon 2", "Weapon 1 Swap", "Weapon 2 Swap", "Helmet",
        "Body Armour", "Gloves", "Boots", "Amulet", "Ring 1", "Ring 2", "Belt",
        "Flask 1", "Flask 2", "Flask 3", "Flask 4", "Flask 5" }
    if tab:ShouldShowRing3(entry) then
        table.insert(baseSlots, 10, "Ring 3")
    end
    tab:AddAbyssSockets(entry, baseSlots, false)
    local function stripCodes(s)
        return (s:gsub("%^x%x%x%x%x%x%x", ""):gsub("%^%d", ""))
    end
    for _, slotName in ipairs(baseSlots) do
        local pSlot = build.itemsTab.slots and build.itemsTab.slots[slotName]
        local cSlot = entry.itemsTab.slots and entry.itemsTab.slots[slotName]
        local pItem = pSlot and build.itemsTab.items[pSlot.selItemId]
        local cItem = cSlot and entry.itemsTab.items[cSlot.selItemId]
        table.insert(out, {
            slot = slotName,
            primary = pItem and pItem.name or "",
            primaryRarity = pItem and pItem.rarity or "",
            compare = cItem and cItem.name or "",
            compareRarity = cItem and cItem.rarity or "",
            status = stripCodes(tradeHelpers.getSlotDiffLabel(pItem, cItem)),
            canCopy = cItem ~= nil,
        })
    end
    for _, jewelEntry in ipairs(tab:GetJewelComparisonSlots(entry)) do
        table.insert(out, {
            slot = jewelEntry.label,
            copySlot = jewelEntry.cSlotName,
            isJewel = true,
            primary = jewelEntry.pItem and jewelEntry.pItem.name or "",
            primaryRarity = jewelEntry.pItem and jewelEntry.pItem.rarity or "",
            compare = jewelEntry.cItem and jewelEntry.cItem.name or "",
            compareRarity = jewelEntry.cItem and jewelEntry.cItem.rarity or "",
            status = stripCodes(tradeHelpers.getSlotDiffLabel(jewelEntry.pItem, jewelEntry.cItem)),
            canCopy = jewelEntry.cItem ~= nil,
            primaryWarn = (jewelEntry.pItem and not jewelEntry.pNodeAllocated) or false,
            compareWarn = (jewelEntry.cItem and not jewelEntry.cNodeAllocated) or false,
        })
    end
    return out
end

function pob_compare.copyItem(index, slotName, andUse)
    local entry = getEntry(index)
    if entry then
        getBuild().compareTab:CopyCompareItemToPrimary(slotName, entry, andUse and true or false)
    end
end

-- Skill comparison: upstream DrawSkills' group pairing (Jaccard similarity
-- over gem-name sets incl. the synthesized imbued entry, greedy best-first)
-- and aligned gem lists with common/additional/missing statuses.
function pob_compare.skillRows(index)
    local build = getBuild()
    local entry = getEntry(index)
    local out = {}
    if not entry then
        return out
    end
    local pSkillsTab = build.skillsTab
    local cSkillsTab = entry.skillsTab
    local function getImbuedGem(group, skillsTab)
        if not group or not group.imbuedSupport then return nil end
        local grantedEffect = skillsTab and skillsTab.imbuedSupportBySlot and group.slot
            and skillsTab.imbuedSupportBySlot[group.slot]
        if not grantedEffect then
            local gemId = data.gemForBaseName
                and data.gemForBaseName[group.imbuedSupport:lower() .. " support"]
            grantedEffect = gemId and data.gems[gemId] and data.gems[gemId].grantedEffect or nil
        end
        return {
            grantedEffect = grantedEffect,
            nameSpec = group.imbuedSupport,
            level = 1,
            quality = 0,
            isImbuedSupport = true,
        }
    end
    local function getGemName(gem)
        return gem.grantedEffect and gem.grantedEffect.name or gem.nameSpec
    end
    local function getGemsWithImbued(group, skillsTab)
        if not group then return {} end
        local gems = {}
        for _, gem in ipairs(group.gemList or {}) do
            table.insert(gems, gem)
        end
        local imbuedGem = getImbuedGem(group, skillsTab)
        if imbuedGem then
            table.insert(gems, imbuedGem)
        end
        return gems
    end
    local function getGemNameSet(group, skillsTab)
        local set = {}
        for _, gem in ipairs(getGemsWithImbued(group, skillsTab)) do
            local name = getGemName(gem)
            if name then
                set[name] = true
            end
        end
        return set
    end
    local function groupSimilarity(setA, setB)
        local intersection = 0
        local union = 0
        local allKeys = {}
        for k in pairs(setA) do allKeys[k] = true end
        for k in pairs(setB) do allKeys[k] = true end
        for k in pairs(allKeys) do
            union = union + 1
            if setA[k] and setB[k] then
                intersection = intersection + 1
            end
        end
        if union == 0 then return 0 end
        return intersection / union
    end

    local pGroups = pSkillsTab.socketGroupList or {}
    local cGroups = cSkillsTab.socketGroupList or {}
    local pSets, cSets = {}, {}
    for i, g in ipairs(pGroups) do pSets[i] = getGemNameSet(g, pSkillsTab) end
    for i, g in ipairs(cGroups) do cSets[i] = getGemNameSet(g, cSkillsTab) end

    local scorePairs = {}
    for pi = 1, #pGroups do
        for ci = 1, #cGroups do
            local score = groupSimilarity(pSets[pi], cSets[ci])
            if score > 0 then
                table.insert(scorePairs, { pIdx = pi, cIdx = ci, score = score })
            end
        end
    end
    -- Deterministic tiebreak on (pIdx, cIdx); upstream's sort is unstable
    table.sort(scorePairs, function(a, b)
        if a.score ~= b.score then return a.score > b.score end
        if a.pIdx ~= b.pIdx then return a.pIdx < b.pIdx end
        return a.cIdx < b.cIdx
    end)
    local pMatched, cMatched, renderPairs = {}, {}, {}
    for _, sp in ipairs(scorePairs) do
        if not pMatched[sp.pIdx] and not cMatched[sp.cIdx] then
            table.insert(renderPairs, { pIdx = sp.pIdx, cIdx = sp.cIdx })
            pMatched[sp.pIdx] = true
            cMatched[sp.cIdx] = true
        end
    end
    for i = 1, #pGroups do
        if not pMatched[i] then table.insert(renderPairs, { pIdx = i, cIdx = nil }) end
    end
    for i = 1, #cGroups do
        if not cMatched[i] then table.insert(renderPairs, { pIdx = nil, cIdx = i }) end
    end

    local function gemEntryOut(entryData)
        return {
            name = entryData.name,
            status = entryData.status,
            level = entryData.gem and entryData.gem.level or 0,
            quality = entryData.gem and entryData.gem.quality or 0,
        }
    end
    local function getGroupLabel(group, idx)
        if not group then return "" end
        local groupLabel = group.displayLabel or group.label or ("Group " .. idx)
        if group.slot then
            groupLabel = groupLabel .. " (" .. group.slot .. ")"
        end
        return groupLabel
    end
    for _, rp in ipairs(renderPairs) do
        local pGroup = rp.pIdx and pGroups[rp.pIdx]
        local cGroup = rp.cIdx and cGroups[rp.cIdx]
        local pSet = rp.pIdx and pSets[rp.pIdx] or {}
        local cSet = rp.cIdx and cSets[rp.cIdx] or {}
        local pDisplay, cDisplay = {}, {}
        local pGems = getGemsWithImbued(pGroup, pSkillsTab)
        local cGems = getGemsWithImbued(cGroup, cSkillsTab)
        local cGemByName = {}
        for _, gem in ipairs(cGems) do
            local name = getGemName(gem)
            if name and pSet[name] and not cGemByName[name] then
                cGemByName[name] = gem
            end
        end
        local emittedCommon = {}
        for _, gem in ipairs(pGems) do
            local name = getGemName(gem)
            if name and cSet[name] and not emittedCommon[name] then
                emittedCommon[name] = true
                table.insert(pDisplay, { gem = gem, name = name, status = "common" })
                table.insert(cDisplay, { gem = cGemByName[name], name = name, status = "common" })
            end
        end
        for _, gem in ipairs(pGems) do
            local name = getGemName(gem)
            if name and not cSet[name] then
                table.insert(pDisplay, { gem = gem, name = name, status = "additional" })
            end
        end
        for _, gem in ipairs(cGems) do
            local name = getGemName(gem)
            if name and not pSet[name] then
                table.insert(cDisplay, { gem = gem, name = name, status = "additional" })
            end
        end
        if pGroup and cGroup then
            local pMissing, cMissing = {}, {}
            for name in pairs(cSet) do
                if not pSet[name] then table.insert(pMissing, name) end
            end
            for name in pairs(pSet) do
                if not cSet[name] then table.insert(cMissing, name) end
            end
            table.sort(pMissing)
            table.sort(cMissing)
            for _, name in ipairs(pMissing) do
                table.insert(pDisplay, { gem = nil, name = name, status = "missing" })
            end
            for _, name in ipairs(cMissing) do
                table.insert(cDisplay, { gem = nil, name = name, status = "missing" })
            end
        end
        local row = {
            primaryLabel = getGroupLabel(pGroup, rp.pIdx or 0),
            compareLabel = getGroupLabel(cGroup, rp.cIdx or 0),
            primaryGems = {},
            compareGems = {},
        }
        for _, e in ipairs(pDisplay) do table.insert(row.primaryGems, gemEntryOut(e)) end
        for _, e in ipairs(cDisplay) do table.insert(row.compareGems, gemEntryOut(e)) end
        table.insert(out, row)
    end
    return out
end

-- Config comparison rows grouped by section: differing values first, then
-- matching relevant values (upstream LayoutConfigView's first pass, without
-- the search filter and live controls).
function pob_compare.configRows(index)
    local build = getBuild()
    local tab = build.compareTab
    local entry = getEntry(index)
    local out = {}
    if not entry then
        return out
    end
    local configVisibility = LoadModule("Modules/ConfigVisibility")
    local configOptions = LoadModule("Modules/ConfigOptions")
    local pInput = build.configTab.input or {}
    local cInput = entry.configTab.input or {}
    local function stripCodes(s)
        return (s:gsub("%^x%x%x%x%x%x%x", ""):gsub("%^%d", ""))
    end
    local currentSection = nil
    for _, varData in ipairs(configOptions) do
        if varData.section then
            if varData.section ~= "Custom Modifiers"
               and varData.section ~= "Map Modifiers and Player Debuffs" then
                currentSection = { name = varData.section, diffs = {}, commons = {} }
                table.insert(out, currentSection)
            else
                currentSection = nil
            end
        elseif currentSection and varData.var and varData.type ~= "text" then
            local pVal, cVal = tab:NormalizeConfigVals(varData,
                pInput[varData.var], cInput[varData.var])
            local row = {
                label = varData.label or varData.var,
                primary = stripCodes(tab:FormatConfigValue(varData, pInput[varData.var])),
                compare = stripCodes(tab:FormatConfigValue(varData, cInput[varData.var])),
            }
            if tostring(pVal) ~= tostring(cVal) then
                table.insert(currentSection.diffs, row)
            else
                local relevant = configVisibility.isRelevantForBuild(varData, build)
                    or configVisibility.isRelevantForBuild(varData, entry)
                if relevant then
                    table.insert(currentSection.commons, row)
                end
            end
        end
    end
    -- Drop empty sections
    for i = #out, 1, -1 do
        if #out[i].diffs == 0 and #out[i].commons == 0 then
            table.remove(out, i)
        end
    end
    return out
end

function pob_compare.copyConfig()
    getBuild().compareTab:CopyCompareConfig()
end

-- Power report driving (upstream RunComparePowerReport / ComparePowerBuilder).
-- statIndex is 1-based into data.powerStatList (0 clears the metric).
function pob_compare.powerSetStat(statIndex, treeNodes, items, skillGems, supportGems, config)
    local tab = getBuild().compareTab
    tab.comparePowerStat = statIndex > 0 and data.powerStatList[statIndex] or nil
    tab.comparePowerCategories = {
        treeNodes = treeNodes and true or false,
        items = items and true or false,
        skillGems = skillGems and true or false,
        supportGems = supportGems and true or false,
        config = config and true or false,
    }
    tab.comparePowerDirty = true
    if not tab.comparePowerStat then
        tab.comparePowerResults = nil
        tab.comparePowerCoroutine = nil
    end
end

function pob_compare.powerStep(index)
    local tab = getBuild().compareTab
    local entry = getEntry(index)
    if not entry then
        return { done = true, progress = 0 }
    end
    tab:RunComparePowerReport(entry)
    return {
        done = tab.comparePowerCoroutine == nil and not tab.comparePowerDirty,
        progress = tab.comparePowerProgress or 0,
    }
end

function pob_compare.powerResults()
    local tab = getBuild().compareTab
    local out = {}
    for _, row in ipairs(tab.comparePowerResults or {}) do
        table.insert(out, {
            category = row.category or "",
            name = (row.name or ""):gsub("%^x%x%x%x%x%x%x", ""):gsub("%^%d", ""),
            impact = row.impact or 0,
            impactStr = (row.combinedImpactStr or row.impactStr or "")
                :gsub("%^x%x%x%x%x%x%x", ""):gsub("%^%d", ""),
            perPoint = row.perPointStr and row.perPointStr
                :gsub("%^x%x%x%x%x%x%x", ""):gsub("%^%d", "") or "",
            pathDist = row.pathDist or 0,
        })
    end
    -- The builder does not sort; upstream's list control defaults to impact
    -- descending
    table.sort(out, function(a, b) return a.impact > b.impact end)
    return out
end

function pob_compare.statRows(index)
    local build = getBuild()
    local entry = build.compareTab.compareEntries[index]
    local rows = {}
    if not entry then
        return rows
    end

    -- Actor/output selection (DrawSummary): use the minion when a build's
    -- main skill is a minion skill
    local primaryCalcs = build.calcsTab
    local compareCalcs = entry.calcsTab
    local primaryEnvMain = primaryCalcs and primaryCalcs.mainEnv
    local compareEnvMain = compareCalcs and compareCalcs.mainEnv
    local primaryMinionSkill = primaryEnvMain and primaryEnvMain.player
        and primaryEnvMain.player.mainSkill
        and primaryEnvMain.player.mainSkill.minion and primaryEnvMain.minion
    local compareMinionSkill = compareEnvMain and compareEnvMain.player
        and compareEnvMain.player.mainSkill
        and compareEnvMain.player.mainSkill.minion and compareEnvMain.minion
    local summaryUseMinion = primaryMinionSkill or compareMinionSkill

    local displayStats = summaryUseMinion and build.minionDisplayStats or build.displayStats
    local primaryOutput = primaryMinionSkill and primaryEnvMain.minion.output
        or primaryCalcs.mainOutput
    local compareOutput = compareMinionSkill and compareEnvMain.minion.output
        or entry:GetOutput()
    if not primaryOutput or not compareOutput or not displayStats then
        return rows
    end
    local primaryActor = primaryMinionSkill and primaryEnvMain.minion
        or (primaryEnvMain and primaryEnvMain.player)
    local compareActor = compareMinionSkill and compareEnvMain.minion
        or (compareEnvMain and compareEnvMain.player)

    local primaryFlags = primaryActor and primaryActor.mainSkill
        and primaryActor.mainSkill.skillFlags or {}
    local compareFlags = compareActor and compareActor.mainSkill
        and compareActor.mainSkill.skillFlags or {}

    for _, statData in ipairs(displayStats) do
        if not statData.stat and not statData.label then
            table.insert(rows, { spacer = true })
        elseif statData.stat == "SkillDPS" then
            -- Skip: multi-row SkillDPS doesn't fit compare layout
        elseif statData.hideStat then
            -- Skip: hidden stats
        elseif not matchFlags(statData.flag, statData.notFlag, primaryFlags)
           and not matchFlags(statData.flag, statData.notFlag, compareFlags) then
            -- Skip: stat not relevant to either build's active skill
        elseif statData.stat then
            local primaryVal = primaryOutput[statData.stat] or 0
            local compareVal = compareOutput[statData.stat] or 0
            if statData.childStat then
                primaryVal = type(primaryVal) == "table" and primaryVal[statData.childStat] or 0
                compareVal = type(compareVal) == "table" and compareVal[statData.childStat] or 0
            end
            if type(primaryVal) == "table" or type(compareVal) == "table" then
                primaryVal = 0
                compareVal = 0
            end
            if (primaryVal ~= 0 or compareVal ~= 0) and
               (not statData.condFunc or statData.condFunc(primaryVal, primaryOutput)
                or statData.condFunc(compareVal, compareOutput)) then
                local fmt = statData.fmt or "d"
                local multiplier = (statData.pc or statData.mod) and 100 or 1
                local primaryStr = formatNumSep(string.format("%"..fmt, primaryVal * multiplier))
                local compareStr = formatNumSep(string.format("%"..fmt, compareVal * multiplier))
                local diff = compareVal - primaryVal
                local diffStr = ""
                local better = 0
                if diff > 0.001 or diff < -0.001 then
                    local isBetter = (statData.lowerIsBetter and diff < 0)
                        or (not statData.lowerIsBetter and diff > 0)
                    better = isBetter and 1 or -1
                    diffStr = formatNumSep(string.format("%+"..fmt, diff * multiplier))
                    if primaryVal ~= 0 then
                        local pc = compareVal / primaryVal * 100 - 100
                        diffStr = diffStr .. string.format(" (%+.1f%%)", pc)
                    end
                end
                table.insert(rows, {
                    label = statData.label or statData.stat,
                    labelColor = statData.color,
                    primaryStr = primaryStr,
                    compareStr = compareStr,
                    diffStr = diffStr,
                    better = better,
                })
            end
        elseif statData.label and statData.condFunc then
            -- Label-only stat (e.g. "Chaos Resistance: Immune")
            if statData.condFunc(primaryOutput) or statData.condFunc(compareOutput) then
                local valStr = statData.val or ""
                table.insert(rows, {
                    label = statData.label,
                    labelColor = statData.color,
                    primaryStr = statData.condFunc(primaryOutput) and valStr or "-",
                    compareStr = statData.condFunc(compareOutput) and valStr or "-",
                    diffStr = "",
                    better = 0,
                })
            end
        end
    end
    return rows
end
