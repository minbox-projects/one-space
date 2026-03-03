import * as React from "react"
import { useEffect, useState } from "react"
import { useTranslation } from "react-i18next"
import { Terminal, Server, Code2, Star, StickyNote } from "lucide-react"
import { invoke } from '@tauri-apps/api/core'
import { emit } from '@tauri-apps/api/event'
import { v4 as uuidv4 } from 'uuid'
import {
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "./ui/command"

interface SearchItem {
  id: string
  title: string
  subtitle?: string
  icon: React.ElementType
  type: 'session' | 'ssh' | 'snippet' | 'bookmark' | 'note'
  action: () => void
}

export function OmniSearch({ open, setOpen }: { open: boolean, setOpen: (o: boolean) => void }) {
  const { t } = useTranslation()
  const [items, setItems] = useState<SearchItem[]>([])

  const isTauri = '__TAURI_INTERNALS__' in window

  useEffect(() => {
    const down = (e: KeyboardEvent) => {
      if (e.key === "k" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault()
        setOpen(true)
      }
    }
    document.addEventListener("keydown", down)
    return () => document.removeEventListener("keydown", down)
  }, [setOpen])

  useEffect(() => {
    if (open) {
      loadAllData()
    }
  }, [open])

  const loadAllData = async () => {
    if (!isTauri) return

    try {
      const newItems: SearchItem[] = []

      // 1. Load Sessions
      try {
        const sessions: any[] = await invoke('get_ai_sessions')
        sessions.forEach(s => {
          newItems.push({
            id: `session-${s.id}`,
            title: s.name,
            subtitle: s.working_dir,
            icon: Terminal,
            type: 'session',
            action: async () => {
              await invoke('launch_native_session', { 
                workingDir: s.working_dir,
                modelType: s.model_type,
                sessionId: s.tool_session_id
              })
              setOpen(false)
            }
          })
        })
      } catch (e) { /* ignore */ }

      // 2. Load SSH Hosts
      try {
        const hosts: any[] = await invoke('get_ssh_hosts')
        hosts.forEach(h => {
          newItems.push({
            id: `ssh-${h.name}`,
            title: h.name,
            subtitle: `${h.user}@${h.host_name}`,
            icon: Server,
            type: 'ssh',
            action: async () => {
              await invoke('connect_ssh', { host: h.name })
              
              // Save to history
              const historyStr = localStorage.getItem('onespace_ssh_history')
              let history = historyStr ? JSON.parse(historyStr) : []
              
              const entry = {
                id: uuidv4(),
                type: 'config',
                name: h.name,
                host_name: h.host_name,
                user: h.user,
                port: h.port,
                last_connected: Date.now(),
                connect_count: 1
              }

              // Update history logic matching SshServers.tsx
              let connectCount = 1
              history = history.filter((old: any) => {
                if (old.name === entry.name) {
                  connectCount = (old.connect_count || 1) + 1
                  return false
                }
                return true
              })
              entry.connect_count = connectCount
              history.unshift(entry)
              localStorage.setItem('onespace_ssh_history', JSON.stringify(history.slice(0, 50)))
              
              // Notify components to refresh history
              emit('refresh-ssh-history')
              
              setOpen(false)
            }
          })
        })
      } catch (e) { /* ignore */ }

      // 3. Load Snippets
      try {
        const snipsStr: string = await invoke('read_snippets')
        const snips: any[] = JSON.parse(snipsStr)
        snips.forEach(s => {
          let subtitle = s.language
          if (s.group) subtitle += ` • ${s.group}`
          if (s.tags && s.tags.length > 0) subtitle += ` • ${s.tags.join(', ')}`

          newItems.push({
            id: `snippet-${s.id}`,
            title: s.title,
            subtitle,
            icon: Code2,
            type: 'snippet',
            action: () => {
              // Write code to clipboard
              navigator.clipboard.writeText(s.code)
              setOpen(false)
            }
          })
        })
      } catch (e) { /* ignore */ }

      // 4. Load Bookmarks
      try {
        const bmsStr: string = await invoke('read_bookmarks')
        const bms: any[] = JSON.parse(bmsStr)
        bms.forEach(b => {
          newItems.push({
            id: `bookmark-${b.id}`,
            title: b.name,
            subtitle: b.url,
            icon: Star,
            type: 'bookmark',
            action: async () => {
              const { open: shellOpen } = await import('@tauri-apps/plugin-shell')
              await shellOpen(b.url)
              setOpen(false)
            }
          })
        })
      } catch (e) { /* ignore */ }

      // 5. Load Notes
      try {
        const notesStr: string = await invoke('read_notes')
        const notes: any[] = JSON.parse(notesStr)
        notes.forEach(n => {
          newItems.push({
            id: `note-${n.id}`,
            title: n.title || 'Untitled Note',
            subtitle: n.content.substring(0, 50).replace(/\n/g, ' '),
            icon: StickyNote,
            type: 'note',
            action: () => {
              // Normally this would navigate to the note.
              // For now, close search.
              setOpen(false)
            }
          })
        })
      } catch (e) { /* ignore */ }

      setItems(newItems)
    } catch (err) {
      console.error(err)
    }
  }

  const groupedItems = items.reduce((acc, item) => {
    if (!acc[item.type]) acc[item.type] = []
    acc[item.type].push(item)
    return acc
  }, {} as Record<string, SearchItem[]>)

  return (
    <CommandDialog open={open} onOpenChange={setOpen}>
      <CommandInput placeholder={t('search')} />
      <CommandList>
        <CommandEmpty>{t('noResultsFound', 'No results found.')}</CommandEmpty>
        
        {groupedItems['session'] && (
          <CommandGroup heading={t('aiSessions')}>
            {groupedItems['session'].map(item => (
              <CommandItem key={item.id} onSelect={item.action}>
                <item.icon className="mr-2 h-4 w-4 text-blue-500" />
                <span>{item.title}</span>
                {item.subtitle && <span className="ml-2 text-xs text-muted-foreground">{item.subtitle}</span>}
              </CommandItem>
            ))}
          </CommandGroup>
        )}

        {groupedItems['ssh'] && (
          <CommandGroup heading={t('sshServers')}>
            {groupedItems['ssh'].map(item => (
              <CommandItem key={item.id} onSelect={item.action}>
                <item.icon className="mr-2 h-4 w-4 text-amber-500" />
                <span>{item.title}</span>
                {item.subtitle && <span className="ml-2 text-xs text-muted-foreground">{item.subtitle}</span>}
              </CommandItem>
            ))}
          </CommandGroup>
        )}

        {groupedItems['bookmark'] && (
          <CommandGroup heading={t('bookmarks')}>
            {groupedItems['bookmark'].map(item => (
              <CommandItem key={item.id} onSelect={item.action}>
                <item.icon className="mr-2 h-4 w-4 text-purple-500" />
                <span>{item.title}</span>
                {item.subtitle && <span className="ml-2 text-xs text-muted-foreground truncate max-w-[200px]">{item.subtitle}</span>}
              </CommandItem>
            ))}
          </CommandGroup>
        )}

        {groupedItems['snippet'] && (
          <CommandGroup heading={t('snippets')}>
            {groupedItems['snippet'].map(item => (
              <CommandItem key={item.id} onSelect={item.action}>
                <item.icon className="mr-2 h-4 w-4 text-green-500" />
                <span>{item.title}</span>
                {item.subtitle && <span className="ml-2 text-xs text-muted-foreground uppercase">{item.subtitle}</span>}
              </CommandItem>
            ))}
          </CommandGroup>
        )}

        {groupedItems['note'] && (
          <CommandGroup heading={t('notes')}>
            {groupedItems['note'].map(item => (
              <CommandItem key={item.id} onSelect={item.action}>
                <item.icon className="mr-2 h-4 w-4 text-rose-500" />
                <span>{item.title}</span>
                {item.subtitle && <span className="ml-2 text-xs text-muted-foreground truncate max-w-[200px]">{item.subtitle}</span>}
              </CommandItem>
            ))}
          </CommandGroup>
        )}
      </CommandList>
    </CommandDialog>
  )
}