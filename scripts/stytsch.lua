-- stytsch.lua — Clink plugin for persistent, searchable command history
-- Requires: fzf (fuzzy finder) and stytsch (history backend) in PATH.

local function is_installed(cmd)
    local f = io.popen('where "' .. cmd .. '" 2>nul')
    if f then
        local path = f:read("*l")
        f:close()
        return path and path ~= ""
    end
    return false
end

local function ensure_fzf()
    if is_installed("fzf") then return true end
    -- Try auto-install via available package managers.
    if is_installed("scoop") then
        print("[stytsch] Installing fzf via scoop...")
        os.execute("scoop install fzf 2>nul")
        if is_installed("fzf") then return true end
    end
    if is_installed("choco") then
        print("[stytsch] Installing fzf via chocolatey...")
        os.execute("choco install fzf -y 2>nul")
        if is_installed("fzf") then return true end
    end
    if is_installed("winget") then
        print("[stytsch] Installing fzf via winget...")
        os.execute("winget install junegunn.fzf --accept-package-agreements --accept-source-agreements 2>nul")
        if is_installed("fzf") then return true end
    end
    print("[stytsch] fzf not found. Install it with one of:")
    print("  scoop install fzf")
    print("  choco install fzf")
    print("  winget install fzf")
    print("  pacman -S $MINGW_PACKAGE_PREFIX-fzf  (MSYS2)")
    return false
end

local fzf_available = ensure_fzf()

local function find_stytsch()
    local f = io.popen("where stytsch 2>nul")
    if f then
        local path = f:read("*l")
        f:close()
        if path and path ~= "" then return path end
    end
    return nil
end

local STYTSCH = find_stytsch()
local stytsch_available = STYTSCH ~= nil

--------------------------------------------------------------------------------
-- Up Arrow / Ctrl+R → fzf search
-- Enter = execute immediately | Right Arrow = paste for editing
-- Typing a new command in the search box and pressing Enter also works.
--------------------------------------------------------------------------------
function stytsch_search(rl_buffer)
    if not fzf_available then rl_buffer:ding(); return end

    local cmd
    if stytsch_available then
        cmd = STYTSCH .. ' search --fzf 2>nul'
    else
        local hfile = os.getenv("LOCALAPPDATA") .. "\\clink\\.history"
        cmd = 'type "' .. hfile .. '" 2>nul | fzf --height=40% --reverse --no-sort 2>nul'
    end

    rl_buffer:beginoutput()
    local f = io.popen(cmd)
    if not f then rl_buffer:ding(); return end
    local result = f:read("*a")
    f:close()

    if result then result = result:gsub("^%s+", ""):gsub("%s+$", "") end
    if result and result ~= "" then
        -- Parse mode prefix: EXEC: = run immediately, EDIT: = paste for editing
        local mode = "edit"
        local command = result
        if result:sub(1, 5) == "EXEC:" then
            mode = "exec"
            command = result:sub(6)
        elseif result:sub(1, 5) == "EDIT:" then
            command = result:sub(6)
        end

        if command ~= "" then
            rl_buffer:beginundogroup()
            rl_buffer:remove(1, rl_buffer:getlength() + 1)
            rl_buffer:insert(command)
            rl_buffer:setcursor(rl_buffer:getlength() + 1)
            rl_buffer:endundogroup()

            if mode == "exec" then
                rl.invokecommand("accept-line")
            end
        end
    end
    rl_buffer:refreshline()
end

rl.describemacro([["luafunc:stytsch_search"]], "Search history with stytsch + fzf")
rl.setbinding([["\e[A"]], [["luafunc:stytsch_search"]])
rl.setbinding([["\C-r"]], [["luafunc:stytsch_search"]])

--------------------------------------------------------------------------------
-- Record commands into stytsch SQLite database
--------------------------------------------------------------------------------
local tracking_enabled = true
local pending_command = nil
local pending_cwd = nil
local pending_start = nil

clink.onendedit(function(input)
    if not tracking_enabled then return end
    if not stytsch_available then return end
    if input and input:match("%S") then
        pending_command = input
        pending_cwd = os.getcwd()
        pending_start = os.time()
    else
        pending_command = nil
    end
end)

clink.onbeginedit(function()
    if not pending_command then return end
    if not stytsch_available then return end

    local exit_code = 0
    if os.geterrorlevel then
        exit_code = os.geterrorlevel() or 0
    end
    local duration = 0
    if pending_start then duration = os.time() - pending_start end

    local temp_dir = os.getenv("TEMP") or os.getenv("TMP") or "."
    local tmp = temp_dir .. "\\stytsch_cmd_" .. os.time() .. ".txt"

    local tf = io.open(tmp, "w")
    if tf then
        tf:write(pending_command)
        tf:close()
        local record_cmd = string.format(
            '%s record --cwd "%s" --exit %d --duration %d --file "%s" 2>&1',
            STYTSCH, pending_cwd or "", exit_code, duration, tmp
        )
        local p = io.popen(record_cmd)
        if p then
            p:read("*a")
            p:close()
        end
        os.remove(tmp)
    end

    pending_command = nil
    pending_cwd = nil
    pending_start = nil
end)

--------------------------------------------------------------------------------
-- Ctrl+Q → toggle tracking
--------------------------------------------------------------------------------
function stytsch_toggle(rl_buffer)
    tracking_enabled = not tracking_enabled
    rl_buffer:beginoutput()
    print(tracking_enabled and "[stytsch: tracking ON]" or "[stytsch: tracking OFF]")
    rl_buffer:refreshline()
end

rl.describemacro([["luafunc:stytsch_toggle"]], "Toggle stytsch tracking")
rl.setbinding([["\C-q"]], [["luafunc:stytsch_toggle"]])
