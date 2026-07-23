-- Helper functions for the Rust calcs tab. Loaded once into the Lua VM.
-- Walks the upstream CalcsTab section list and formats cells the same way
-- CalcSectionControl:FormatStr does, returning plain nested tables that the
-- Rust side can consume. Breakdown resolution mirrors CalcBreakdownControl.

local function getCalcsTab()
    local build = mainObject_ref.main.modes['BUILD']
    local calcsTab = build.calcsTab
    if not calcsTab.calcsEnv then
        calcsTab:BuildOutput()
    end
    return calcsTab
end

local function getActor(calcsTab)
    return (calcsTab.input.showMinion and calcsTab.calcsEnv.minion) or calcsTab.calcsEnv.player
end

local function formatVal(val, p)
    return formatNumSep(tostring(round(val, p)))
end

-- Port of CalcSectionControl:FormatStr
local function formatStr(str, actor, colData)
    str = str:gsub("{output:([%a%.:]+)}", function(c)
        local ns, var = c:match("^(%a+)%.(%a+)$")
        if ns then
            return actor.output[ns] and actor.output[ns][var] or ""
        else
            return actor.output[c] or ""
        end
    end)
    str = str:gsub("{(%d+):output:([%a%.:]+)}", function(p, c)
        local ns, var = c:match("^(%a+)%.(%a+)$")
        if ns then
            return formatVal(actor.output[ns] and actor.output[ns][var] or 0, tonumber(p))
        else
            return formatVal(actor.output[c] or 0, tonumber(p))
        end
    end)
    str = str:gsub("{(%d+):mod:([%d,]+)}", function(p, n)
        local numList = { }
        for num in n:gmatch("%d+") do
            table.insert(numList, tonumber(num))
        end
        local modType = colData[numList[1]].modType
        local modTotal = modType == "MORE" and 1 or 0
        for _, num in ipairs(numList) do
            local sectionData = colData[num]
            local modCfg = (sectionData.cfg and actor.mainSkill[sectionData.cfg.."Cfg"]) or { }
            if sectionData.modSource then
                modCfg.source = sectionData.modSource
            end
            if sectionData.actor then
                modCfg.actor = sectionData.actor
            end
            local modVal
            local modStore = (sectionData.enemy and actor.enemy.modDB) or (sectionData.cfg and actor.mainSkill.skillModList) or actor.modDB
            if type(sectionData.modName) == "table" then
                modVal = modStore:Combine(sectionData.modType, modCfg, unpack(sectionData.modName))
            else
                modVal = modStore:Combine(sectionData.modType, modCfg, sectionData.modName)
            end
            if modType == "MORE" then
                modTotal = modTotal * modVal
            else
                modTotal = modTotal + modVal
            end
        end
        if modType == "MORE" then
            modTotal = (modTotal - 1) * 100
        end
        return formatVal(modTotal, tonumber(p))
    end)
    return str
end

-- Extract all visible sections with formatted cell text.
function pob_calcs_extract()
    local calcsTab = getCalcsTab()
    local actor = getActor(calcsTab)
    local sections = {}
    for si, section in ipairs(calcsTab.sectionList) do
        local secOut = {
            si = si,
            id = section.id or "",
            group = section.group or 1,
            colour = section.colour or "^7",
            subsections = {},
        }
        for ui, subSec in ipairs(section.subSection) do
            if calcsTab:CheckFlag(subSec.data) then
                local sub = {
                    ui = ui,
                    label = subSec.label or "",
                    collapsed = subSec.defaultCollapsed or false,
                    rows = {},
                }
                if subSec.data.extra then
                    local ok, text = pcall(formatStr, subSec.data.extra, actor)
                    sub.extra = ok and text or "?"
                end
                for ri, rowData in ipairs(subSec.data) do
                    if calcsTab:CheckFlag(rowData) then
                        local row = { ri = ri, label = rowData.label, cells = {} }
                        local anyText = false
                        for ci, colData in ipairs(rowData) do
                            local text = ""
                            if colData.format then
                                local ok, formatted = pcall(formatStr, colData.format, actor, colData)
                                text = ok and formatted or "?"
                            end
                            if text ~= "" then
                                anyText = true
                            end
                            table.insert(row.cells, {
                                ci = ci,
                                text = text,
                                hasBreakdown = colData[1] ~= nil,
                            })
                        end
                        if anyText or row.label then
                            table.insert(sub.rows, row)
                        end
                    end
                end
                if #sub.rows > 0 then
                    table.insert(secOut.subsections, sub)
                end
            end
        end
        if #secOut.subsections > 0 then
            table.insert(sections, secOut)
        end
    end
    return sections
end

-- Port of CalcBreakdownControl:FormatModValue
local function formatModValue(value, modType)
    if modType == "BASE" then
        return string.format("%+g base", value)
    elseif modType == "INC" then
        if value >= 0 then
            return value.."% increased"
        else
            return -value.."% reduced"
        end
    elseif modType == "MORE" then
        if value >= 0 then
            return value.."% more"
        else
            return -value.."% less"
        end
    elseif modType == "OVERRIDE" then
        return "Override: "..tostring(value)
    elseif modType == "FLAG" then
        return value and "True" or "False"
    elseif modType == "LIST" then
        if type(value) == "table" and value.mod then
            return "Modifier: "..value.mod.name
        else
            return "?"
        end
    else
        return tostring(value)
    end
end

local function cellString(v)
    if v == nil then
        return ""
    end
    if type(v) == "number" then
        return formatNumSep(tostring(v))
    end
    return tostring(v)
end

-- Convert a rowList/colList pair into { cols = {labels}, rows = {{values}} }
local function flattenTable(label, footer, colList, rowList)
    local out = { type = "TABLE", label = label, footer = footer, cols = {}, rows = {} }
    for _, col in ipairs(colList) do
        table.insert(out.cols, col.label or "")
    end
    for _, row in ipairs(rowList) do
        local flat = {}
        for _, col in ipairs(colList) do
            table.insert(flat, cellString(row[col.key]))
        end
        table.insert(out.rows, flat)
    end
    return out
end

-- Simplified port of CalcBreakdownControl:AddModSection
local function addModSection(sections, calcsTab, actor, sectionData, modList)
    local cfg = (sectionData.cfg and actor.mainSkill[sectionData.cfg.."Cfg"] and copyTable(actor.mainSkill[sectionData.cfg.."Cfg"], true)) or { }
    cfg.source = sectionData.modSource
    cfg.actor = sectionData.actor
    local rowList
    local modStore = (sectionData.enemy and actor.enemy.modDB) or (sectionData.cfg and actor.mainSkill.skillModList) or actor.modDB
    if modList then
        rowList = copyTable(modList)
    else
        if type(sectionData.modName) == "table" then
            rowList = modStore:Tabulate(sectionData.modType, cfg, unpack(sectionData.modName))
        else
            rowList = modStore:Tabulate(sectionData.modType, cfg, sectionData.modName)
        end
    end
    if #rowList == 0 then
        return
    end
    table.sort(rowList, function(a, b)
        if a.mod.type ~= b.mod.type then
            return a.mod.type < b.mod.type
        end
        if a.mod.name ~= b.mod.name then
            return a.mod.name < b.mod.name
        end
        if type(a.value) == "number" and type(b.value) == "number" then
            return a.value > b.value
        end
        return false
    end)
    local out = {
        type = "TABLE",
        label = sectionData.label or "Modifiers",
        cols = { "Value", "Stat", "Source", "Source Name" },
        rows = {},
    }
    for _, row in ipairs(rowList) do
        local sourceType, sourceName = "", ""
        if row.mod.source then
            sourceType = row.mod.source:match("^([^:]+)") or row.mod.source
            sourceName = row.mod.source:match("^[^:]+:(.+)$") or ""
        end
        -- Item sources encode "Item:<id>:<name>"
        local itemName = sourceName:match("^%d+:(.+)$")
        if itemName then
            sourceName = itemName
        end
        table.insert(out.rows, {
            formatModValue(row.value, row.mod.type),
            row.mod.name:gsub("(%l)(%u)", "%1 %2"),
            sourceType,
            sourceName,
        })
    end
    table.insert(sections, out)
end

-- Port of CalcBreakdownControl:AddBreakdownSection (minus the radius visual)
local function addBreakdownSection(sections, calcsTab, actor, sectionData)
    local breakdown
    local ns, name = sectionData.breakdown:match("^(%a+)%.(%a+)$")
    if ns then
        breakdown = actor.breakdown[ns] and actor.breakdown[ns][name]
    else
        breakdown = actor.breakdown[sectionData.breakdown]
    end
    if not breakdown then
        return
    end

    if #breakdown > 0 then
        local lines = {}
        for _, line in ipairs(breakdown) do
            table.insert(lines, tostring(line))
        end
        table.insert(sections, { type = "TEXT", lines = lines })
    end

    if breakdown.rowList and #breakdown.rowList > 0 then
        local rowList = copyTable(breakdown.rowList, true)
        local colKey = breakdown.colList[1].key
        if breakdown.source then
            table.sort(rowList, function(a, b)
                if a.reqNum then
                    return a.reqNum > b.reqNum
                end
                return a[colKey] > b[colKey]
            end)
        end
        table.insert(sections, flattenTable(breakdown.label, breakdown.footer, breakdown.colList, rowList))
    end

    if breakdown.reservations and #breakdown.reservations > 0 then
        table.insert(sections, flattenTable(nil, nil, {
            { label = "Skill", key = "skillName" },
            { label = "Base", key = "base" },
            { label = "MCM", key = "mult" },
            { label = "More/less", key = "more" },
            { label = "Inc/red", key = "inc" },
            { label = "Efficiency", key = "efficiency" },
            { label = "Reservation", key = "total" },
        }, breakdown.reservations))
    end

    if breakdown.damageTypes and #breakdown.damageTypes > 0 then
        table.insert(sections, flattenTable(nil, nil, {
            { label = "From", key = "source" },
            { label = "Base", key = "base" },
            { label = "Inc/red", key = "inc" },
            { label = "More/less", key = "more" },
            { label = "Converted Damage", key = "convSrc" },
            { label = "Total", key = "total" },
            { label = "Conversion", key = "convDst" },
            { label = "Gain", key = "gainDst" },
        }, breakdown.damageTypes))
    end

    if breakdown.slots and #breakdown.slots > 0 then
        local rowList = copyTable(breakdown.slots, true)
        table.sort(rowList, function(a, b)
            return a.base > b.base
        end)
        for _, row in ipairs(rowList) do
            if row.item then
                row.sourceLabel = row.item.name
            else
                row.sourceLabel = row.sourceName
            end
        end
        table.insert(sections, flattenTable(nil, nil, {
            { label = "Base", key = "base" },
            { label = "Inc/red", key = "inc" },
            { label = "More/less", key = "more" },
            { label = "Total", key = "total" },
            { label = "Source", key = "source" },
            { label = "Name", key = "sourceLabel" },
        }, rowList))
    end

    if breakdown.modList and #breakdown.modList > 0 then
        addModSection(sections, calcsTab, actor, sectionData, breakdown.modList)
    end
end

-- Resolve the breakdown for a cell addressed by original indices.
function pob_calcs_breakdown(si, ui, ri, ci)
    local calcsTab = getCalcsTab()
    local actor = getActor(calcsTab)
    local section = calcsTab.sectionList[si]
    if not section then return {} end
    local subSec = section.subSection[ui]
    if not subSec then return {} end
    local rowData = subSec.data[ri]
    if not rowData then return {} end
    local colData = rowData[ci]
    if not colData then return {} end

    local sections = {}
    for _, sectionData in ipairs(colData) do
        if calcsTab:CheckFlag(sectionData) then
            if sectionData.breakdown then
                local ok, err = pcall(addBreakdownSection, sections, calcsTab, actor, sectionData)
                if not ok then
                    table.insert(sections, { type = "TEXT", lines = { "Breakdown error: "..tostring(err) } })
                end
            elseif sectionData.modName then
                local ok, err = pcall(addModSection, sections, calcsTab, actor, sectionData)
                if not ok then
                    table.insert(sections, { type = "TEXT", lines = { "Breakdown error: "..tostring(err) } })
                end
            end
        end
    end
    return sections
end

-- Current calcs input state and available options.
function pob_calcs_get_input(...)
    local calcsTab = getCalcsTab()
    return {
        skillNumber = calcsTab.input.skill_number or 1,
        buffMode = calcsTab.input.misc_buffMode or "EFFECTIVE",
        showMinion = calcsTab.input.showMinion or false,
        hasMinion = calcsTab.calcsEnv.minion ~= nil,
    }
end

-- Set a calcs input field and trigger a recalc.
function pob_calcs_set_input(key, value)
    local build = mainObject_ref.main.modes['BUILD']
    build.calcsTab.input[key] = value
    build.buildFlag = true
    _runCallback('OnFrame')
end
