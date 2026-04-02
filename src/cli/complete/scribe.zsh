#compdef scribe

# Scribe zsh completion.
# All helpers defined inside _scribe so autoload works correctly.

_scribe() {
  # ── dynamic slug helpers ────────────────────────────────────────────────
  _scribe_dynamic_complete() {
    local -a candidates
    local slug hint
    while IFS=$'\t' read -r slug hint; do
      candidates+=("${slug}:${hint}")
    done < <(scribe __complete "$1" 2>/dev/null)
    _describe 'slug' candidates
  }
  _scribe_complete_projects()  { _scribe_dynamic_complete projects  }
  _scribe_complete_tasks()     { _scribe_dynamic_complete tasks     }
  _scribe_complete_todos()     { _scribe_dynamic_complete todos     }
  _scribe_complete_reminders() { _scribe_dynamic_complete reminders }
  _scribe_complete_captures()  { _scribe_dynamic_complete captures  }

  # ── leaf argument specs ──────────────────────────────────────────────────
  # Called after words/CURRENT are already shifted to the sub-subcommand level.
  # $words[1] = sub-subcommand (e.g. "show"), CURRENT counts from there.

  _scribe_args_project() {
    case $words[1] in
    (add)
      _arguments \
        '--name=[Project name]:name: ' \
        '--desc=[Description]:desc: ' \
        '--output=[Output format]:format:(text json)' \
        ':slug: ' ;;
    (list)
      _arguments \
        '--status=[Status]:status:(active paused completed)' \
        '--output=[Output format]:format:(text json)' \
        '--archived[Include archived]' ;;
    (show|edit|archive|restore|delete)
      _arguments \
        '--output=[Output format]:format:(text json)' \
        ':slug:_scribe_complete_projects' ;;
    esac
  }

  _scribe_args_task() {
    case $words[1] in
    (add)
      _arguments \
        '--project=[Project slug]:project:_scribe_complete_projects' \
        '--priority=[Priority]:priority:(low medium high urgent)' \
        '--due=[Due date]:due: ' \
        '--output=[Output format]:format:(text json)' \
        ':title: ' ;;
    (list)
      _arguments \
        '--project=[Project slug]:project:_scribe_complete_projects' \
        '--status=[Status]:status:(todo in_progress done cancelled)' \
        '--priority=[Priority]:priority:(low medium high urgent)' \
        '--output=[Output format]:format:(text json)' \
        '--archived[Include archived]' ;;
    (move)
      _arguments \
        '--project=[Destination project]:project:_scribe_complete_projects' \
        '--output=[Output format]:format:(text json)' \
        ':slug:_scribe_complete_tasks' ;;
    (show|done|archive|restore|delete)
      _arguments \
        '--output=[Output format]:format:(text json)' \
        ':slug:_scribe_complete_tasks' ;;
    (edit)
      _arguments \
        '--title=[New title]:title: ' \
        '--status=[New status]:status:(todo in_progress done cancelled)' \
        '--priority=[New priority]:priority:(low medium high urgent)' \
        '--due=[New due date]:due: ' \
        '--project=[New project slug]:project:_scribe_complete_projects' \
        '--output=[Output format]:format:(text json)' \
        ':slug:_scribe_complete_tasks' ;;
    esac
  }

  _scribe_args_todo() {
    case $words[1] in
    (add)
      _arguments \
        '--project=[Project slug]:project:_scribe_complete_projects' \
        '--output=[Output format]:format:(text json)' \
        ':title: ' ;;
    (list)
      _arguments \
        '--project=[Project slug]:project:_scribe_complete_projects' \
        '--output=[Output format]:format:(text json)' \
        '--all[Include done todos]' \
        '--archived[Show archived]' ;;
    (move)
      _arguments \
        '--project=[Destination project]:project:_scribe_complete_projects' \
        '--output=[Output format]:format:(text json)' \
        ':slug:_scribe_complete_todos' ;;
    (show|done|archive|restore|delete)
      _arguments \
        '--output=[Output format]:format:(text json)' \
        ':slug:_scribe_complete_todos' ;;
    esac
  }

  _scribe_args_track() {
    case $words[1] in
    (start)
      _arguments \
        '--project=[Project slug]:project:_scribe_complete_projects' \
        '--task=[Task slug]:task:_scribe_complete_tasks' \
        '--note=[Note]:note: ' \
        '--output=[Output format]:format:(text json)' ;;
    (stop|status)
      _arguments '--output=[Output format]:format:(text json)' ;;
    (report)
      _arguments \
        '--project=[Project slug]:project:_scribe_complete_projects' \
        '--output=[Output format]:format:(text json)' \
        '--today[Restrict to today]' \
        '--week[Restrict to this week]' ;;
    esac
  }

  _scribe_args_inbox() {
    case $words[1] in
    (list)
      _arguments \
        '--output=[Output format]:format:(text json)' \
        '--all[Include processed items]' ;;
    (process)
      _arguments \
        '--output=[Output format]:format:(text json)' \
        ':slug:_scribe_complete_captures' ;;
    esac
  }

  _scribe_args_reminder() {
    case $words[1] in
    (add)
      _arguments \
        '--project=[Project slug]:project:_scribe_complete_projects' \
        '--at=[When to fire]:at: ' \
        '--task=[Task slug]:task:_scribe_complete_tasks' \
        '--message=[Message]:message: ' \
        '--persistent[Stay until dismissed (blocking alert)]' \
        '--output=[Output format]:format:(text json)' ;;
    (list)
      _arguments \
        '--project=[Project slug]:project:_scribe_complete_projects' \
        '--output=[Output format]:format:(text json)' \
        '--archived[Include archived]' ;;
    (show|archive|restore|delete)
      _arguments \
        '--output=[Output format]:format:(text json)' \
        ':slug:_scribe_complete_reminders' ;;
    esac
  }

  # ── subcommand listers ───────────────────────────────────────────────────
  _scribe_list_project_subs() {
    local -a s; s=(
      'add:Create a new project'   'list:List projects'
      'show:Show a project'        'edit:Edit a project'
      'archive:Archive a project'  'restore:Restore an archived project'
      'delete:Delete a project'
    )
    _describe -t commands 'project subcommands' s
  }
  _scribe_list_task_subs() {
    local -a s; s=(
      'add:Create a new task'    'list:List tasks'
      'show:Show a task'         'edit:Edit a task'
      'move:Move to a project'   'done:Mark as done'
      'archive:Archive a task'   'restore:Restore a task'
      'delete:Delete a task'
    )
    _describe -t commands 'task subcommands' s
  }
  _scribe_list_todo_subs() {
    local -a s; s=(
      'add:Create a new todo'   'list:List todos'
      'show:Show a todo'        'move:Move to a project'
      'done:Mark as done'       'archive:Archive a todo'
      'restore:Restore a todo'  'delete:Delete a todo'
    )
    _describe -t commands 'todo subcommands' s
  }
  _scribe_list_track_subs() {
    local -a s; s=(
      'start:Start a new timer'  'stop:Stop the running timer'
      'status:Show timer status' 'report:Show a time report'
    )
    _describe -t commands 'track subcommands' s
  }
  _scribe_list_inbox_subs() {
    local -a s; s=(
      'list:List unprocessed inbox items'
      'process:Process an inbox item interactively'
    )
    _describe -t commands 'inbox subcommands' s
  }
  _scribe_list_reminder_subs() {
    local -a s; s=(
      'add:Create a new reminder'    'list:List reminders'
      'show:Show a reminder'         'archive:Archive a reminder'
      'restore:Restore a reminder'   'delete:Delete a reminder'
    )
    _describe -t commands 'reminder subcommands' s
  }

  # ── top-level dispatch ───────────────────────────────────────────────────
  local context state state_descr line
  local -A opt_args

  local -a top_cmds; top_cmds=(
    'project:Manage projects'
    'task:Manage tasks'
    'todo:Manage todos'
    'track:Time tracking'
    'capture:Quickly capture a thought into the inbox'
    'inbox:Manage the quick-capture inbox'
    'reminder:Manage reminders'
    'setup:First-run wizard and setup status'
    'service:Manage the background daemon service'
    'daemon:Run the background reminder daemon'
    'sync:Sync state to or from a remote provider'
    'agent:Install skill files for AI coding agents'
    'completions:Print a shell completion script'
    'help:Print help'
  )

  _arguments -C \
    '(-h --help)'{-h,--help}'[Print help]' \
    '(-V --version)'{-V,--version}'[Print version]' \
    ': :->cmd' \
    '*:: :->args' \
    && return

  case $state in
  (cmd)
    _describe -t commands 'scribe commands' top_cmds
    ;;
  (args)
    # After *:: shift: $words[1]=top-cmd, $words[2..]=its args, CURRENT counts from 1.
    # We need to dispatch based on $words[1] (the top-level subcommand).
    # For the sub-subcommand level: shift words again so $words[1]=sub-subcommand.
    local top=$words[1]
    shift words
    (( CURRENT-- ))

    case $top in
    (project)
      if (( CURRENT == 1 )); then
        _scribe_list_project_subs
      else
        _scribe_args_project
      fi ;;
    (task)
      if (( CURRENT == 1 )); then
        _scribe_list_task_subs
      else
        _scribe_args_task
      fi ;;
    (todo)
      if (( CURRENT == 1 )); then
        _scribe_list_todo_subs
      else
        _scribe_args_todo
      fi ;;
    (track)
      if (( CURRENT == 1 )); then
        _scribe_list_track_subs
      else
        _scribe_args_track
      fi ;;
    (capture)
      _arguments \
        '--output=[Output format]:format:(text json)' \
        ':text: ' ;;
    (inbox)
      if (( CURRENT == 1 )); then
        _scribe_list_inbox_subs
      else
        _scribe_args_inbox
      fi ;;
    (reminder)
      if (( CURRENT == 1 )); then
        _scribe_list_reminder_subs
      else
        _scribe_args_reminder
      fi ;;
    (agent)
      if (( CURRENT == 1 )); then
        local -a s; s=('install:Install the Scribe skill file to all detected agent directories')
        _describe -t commands 'agent subcommands' s
      else
        _arguments '--output=[Output format]:format:(text json)'
      fi ;;
    (sync)
      if (( CURRENT == 1 )); then
        local -a s; s=(
          'configure:Configure the sync provider and store secrets in the keychain'
          'status:Show sync status'
        )
        _describe -t commands 'sync subcommands' s
      else
        case $words[1] in
        (configure)
          _arguments \
            '--provider=[Sync provider]:provider:(gist s3 icloud jsonbin dropbox rest file)' \
            '--remove[Remove stored keychain secrets for the active provider]' \
            '--output=[Output format]:format:(text json)' ;;
        (status)
          _arguments '--output=[Output format]:format:(text json)' ;;
        (*)
          _arguments '--output=[Output format]:format:(text json)' ;;
        esac
      fi ;;
    (setup)
      _arguments \
        '--wizard[Always run the interactive wizard]' \
        '--status[Show setup status and exit]' ;;
    (service)
      if (( CURRENT == 1 )); then
        local -a s; s=(
          'install:Install and start the background daemon service'
          'uninstall:Stop and remove the background daemon service'
          'status:Show whether the daemon service is installed'
        )
        _describe -t commands 'service subcommands' s
      fi ;;
    (daemon)
      _arguments '--interval=[Polling interval in seconds]:seconds: ' ;;
    (completions)
      _values 'shell' bash zsh fish elvish powershell ;;
    esac ;;
  esac
}

_scribe "$@"
