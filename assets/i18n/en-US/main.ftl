# Navigation
nav-dashboard = Dashboard
nav-servers = Game Servers
nav-steamcmd = SteamCMD
nav-commands = Commands
nav-steam-config = Steam Credentials
nav-autostart = Autostart
nav-shutdown = Shutdown

hello-world = Hello World!
greeting = Hello { $name }!
        .placeholder = Hello Friend!
about = About
simple = simple text
reference = simple text with a reference: { -something }
parameter = text with a { $param }
parameter2 = text one { $param } second { $multi-word-param }
email = text with an EMAIL("example@example.org")
fallback = this should fall back

# Commands
commands-title = Command Runs
commands-back = Back to list
commands-no-runs = No command runs found.
commands-table-id = ID
commands-table-command = Command
commands-table-status = Status
commands-table-exit = Exit
commands-table-title = Title
commands-table-started = Started
commands-detail-title = Command Run #{$id}
commands-detail-title-field = Title
commands-detail-command = Command
commands-detail-status = Status
commands-detail-exit-code = Exit Code
commands-detail-started = Started
commands-detail-completed = Completed
commands-detail-working-dir = Working Dir
commands-detail-pid = PID
commands-detail-arguments = Arguments
commands-detail-log-output = Log Output
commands-detail-live = Live
commands-detail-completed-status = Run completed with status: { $status }
commands-placeholder = —

# Status badges
status-running = Running
status-finished = Finished
status-failed = Failed
status-stopped = Stopped
exit-success = Success
exit-failed = Failed
exit-none = —

# Game Servers
servers-title = Game Servers
servers-back = Back to list
servers-new = New Server
servers-empty = No game servers found. Install your first server to get started.
servers-table-name = Name
servers-table-app-id = App ID
servers-table-platform = Platform
servers-table-status = Status
servers-table-install-dir = Install Dir
servers-detail-title = Game Server
servers-info = Server Info
servers-name = Server Name
servers-app-id = App ID
servers-platform = Platform
servers-install-dir = Install Directory
servers-mod = Mod
servers-beta-branch = Beta Branch
servers-last-error = Last Error
servers-actions = Actions
servers-start = Start Server
servers-stop = Stop Server
servers-update = Update Game
servers-delete = Delete Server
servers-delete-confirm = Are you sure you want to delete this server? This will not remove installed files.
servers-boot-script = Boot Script
servers-boot-script-help = Shell command or script to start the game server. Leave empty to auto-detect.
servers-save-boot-script = Save Boot Script
servers-new-title = Install Game Server
servers-form-app-id = Steam App ID
servers-form-app-id-help = The Steam App ID for the game server (e.g., 730 for CS2, 740 for CS:GO).
servers-form-app-id-link-text = Find app IDs at
servers-form-name = Server Name
servers-form-name-placeholder = My CS2 Server
servers-form-platform = Target Platform
servers-form-mod = Server Mod (optional)
servers-form-mod-placeholder = czero
servers-form-mod-help = Mod name for Half-Life 1 games (e.g., czero, dmc, tfc).
servers-form-beta = Beta Branch (optional)
servers-form-beta-placeholder = public
servers-form-install = Install Server
servers-form-use-steam-account = Use Steam Account Login
servers-form-use-steam-account-help = When checked, SteamCMD will log in with your Steam credentials. Unchecked uses anonymous login.
servers-form-steam-username = Steam Username
servers-form-steam-username-placeholder = Your Steam account username
servers-form-steam-password = Steam Password
servers-form-steam-password-placeholder = Your Steam account password
servers-form-boot-script = Boot Script (optional)
servers-form-boot-script-placeholder = e.g. ./srcds_run -game csgo +map de_dust2
servers-form-boot-script-help = Shell command or script to start the game server. Leave empty to auto-detect.
servers-form-auto-start = Auto-Start
servers-form-auto-start-help = Start the server when Game Smith boots.
servers-form-auto-restart = Auto-Restart
servers-form-auto-restart-help = Restart the server if it crashes.
servers-form-auto-update = Auto-Update
servers-form-auto-update-help = Update the server automatically on new releases.
servers-form-update-on-start = Update On Start
servers-form-update-on-start-help = Update the server before starting.
servers-form-restart-schedule = Restart Schedule (optional)
servers-form-restart-schedule-placeholder = e.g. 0 0 * * *
servers-form-restart-schedule-help = Cron expression for scheduled restarts.
servers-form-template-note = Fields are pre-filled from the source template. Changes override template defaults.
servers-table-login-mode = Login
servers-login-anonymous = Anonymous
servers-login-steam = Steam Account
servers-command-output = Command Output
servers-command-output-help = Live output from the latest command run for this server.
servers-command-run-id = Run

# Additional status badges
status-pending = Pending
status-installing = Installing
status-installed = Installed
status-error = Error

# Server settings
servers-edit-settings = Server Settings
servers-save-settings = Save Settings
servers-settings = Settings
servers-auto-restart = Auto-Restart
servers-auto-restart-help = Automatically restart the server if it crashes.

# Server auto-start
servers-auto-start = Auto-Start
servers-auto-start-help = Automatically start the server when Game Smith boots.
servers-table-auto-start = Auto-Start

servers-steam-creds-required = Steam credentials are not configured. 
servers-steam-creds-link = Set up Steam Credentials
servers-running-locked = Settings are locked while the server is running.

# Game Templates
templates-title = Templates
templates-back = Back to list
templates-back-to-detail = Back to template
templates-new = New Template
templates-import = Import
templates-empty = No templates found. Create or import one to get started.
templates-new-title = New Template
templates-edit-title = Edit Template
templates-import-title = Import Template
templates-details = Template Details
templates-settings = Settings
templates-actions = Actions
templates-edit = Edit
templates-delete = Delete
templates-delete-confirm = Are you sure you want to delete this template?
templates-copy = Copy
templates-export = Export
templates-export-help = Copy this string to import the template on another machine.
templates-name = Name
templates-description = Description
templates-app-id = App ID
templates-server-mod = Server Mod
templates-beta-branch = Beta Branch
templates-login-mode = Login Mode
templates-auto-start = Auto-Start
templates-auto-restart = Auto-Restart
templates-auto-update = Auto-Update
templates-update-on-start = Update on Start
templates-restart-schedule = Restart Schedule
templates-boot-script = Boot Script

# Template form labels
templates-form-name = Template Name
templates-form-name-placeholder = My CS2 Template
templates-form-description = Description (optional)
templates-form-description-placeholder = Brief description of this template
templates-form-app-id = Steam App ID
templates-form-app-id-help = The Steam App ID for the game server.
templates-form-app-id-link-text = Find app IDs at
templates-form-mod = Server Mod (optional)
templates-form-mod-placeholder = czero
templates-form-mod-help = Mod name for Half-Life 1 games.
templates-form-beta = Beta Branch (optional)
templates-form-beta-placeholder = public
templates-form-boot-script = Boot Script (optional)
templates-form-boot-script-placeholder = # e.g. ./srcds_run -game csgo +map de_dust2
templates-form-boot-script-help = Shell command or script to start the game server. Leave empty to auto-detect.
templates-form-use-steam-login = Use Steam Account Login
templates-form-use-steam-login-help = When checked, the server will use Steam credentials for login.
templates-form-auto-start = Auto-Start
templates-form-auto-restart = Auto-Restart
templates-form-auto-update = Auto-Update
templates-form-update-on-start = Update on Start
templates-form-restart-schedule = Restart Schedule (optional)
templates-form-restart-schedule-help = Cron schedule for periodic restarts (e.g., "0 4 * * *" for 4 AM daily).
templates-form-create = Create Template
templates-form-save = Save Changes
templates-cancel = Cancel

# Template import
templates-import-data-label = Template Data
templates-import-data-placeholder = Paste the exported template string here...
templates-import-help = Paste a base64-encoded template string exported from another machine.
templates-import-paste = Decode
templates-import-preview = Template Preview
templates-import-create = Create Template
templates-import-back = Back

# Template table columns
templates-table-name = Name
templates-table-app-id = App ID
templates-table-mod = Mod
templates-table-beta = Beta Branch
templates-table-login = Login

# Navigation
nav-templates = Templates

# Save as Template
servers-save-as-template = Save as Template

# Auto Settings section
templates-auto-settings = Auto Settings
templates-edit-settings = Template Settings

# Server creation workflow
servers-create-new-server = Create New Server
servers-create-new-server-desc = Start from scratch and configure every setting manually.
servers-create-from-template = Create from Template
servers-create-from-template-desc = Pick a template to pre-fill server settings.
templates-use-as-server = Use this template
templates-table-actions = Actions

# Select template page
servers-select-template-title = Choose a Template
servers-select-template-subtitle = Select a template to pre-fill the installation settings. You can change everything before installing.
servers-select-template-empty = No templates available.
servers-select-template-create-one = Create a Template
servers-select-template-app-id = App ID
servers-select-template-mod = Mod
servers-select-template-beta = Beta Branch
servers-select-template-login = Login
servers-select-template-use = Use this template
