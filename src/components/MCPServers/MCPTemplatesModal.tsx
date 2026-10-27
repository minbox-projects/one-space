import { useTranslation } from 'react-i18next';
import { ChevronRight, Key, X } from 'lucide-react';
import type { MCPTemplate } from './types';

interface MCPTemplatesModalProps {
  templates: MCPTemplate[];
  onClose: () => void;
  onSelect: (template: MCPTemplate) => void;
}

export function MCPTemplatesModal({ templates, onClose, onSelect }: MCPTemplatesModalProps) {
  const { t } = useTranslation();
  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4">
      <div className="bg-background rounded-lg w-full max-w-3xl max-h-[90vh] overflow-y-auto">
        <div className="p-6 border-b flex justify-between items-center sticky top-0 bg-background z-10">
          <div>
            <h3 className="text-xl font-bold">{t('chooseTemplate')}</h3>
            <p className="text-sm text-muted-foreground mt-1">
              {t('chooseTemplateDesc')}
            </p>
          </div>
          <button onClick={onClose} className="p-2 hover:bg-secondary rounded">
            <X className="w-5 h-5" />
          </button>
        </div>
        
        <div className="p-6 grid gap-4">
          {templates.map(template => (
            <div 
              key={template.id}
              className="border rounded-lg p-4 hover:bg-accent/50 cursor-pointer transition-colors"
              onClick={() => onSelect(template)}
            >
              <div className="flex justify-between items-start">
                <div>
                  <h4 className="font-semibold text-lg">{template.name}</h4>
                  <p className="text-sm text-muted-foreground mt-1">{template.description}</p>
                  
                  <div className="flex items-center gap-3 mt-3">
                    <span className="text-xs px-2 py-1 bg-secondary rounded uppercase">
                      {template.transport}
                    </span>
                    {template.command && (
                      <code className="text-xs bg-muted px-2 py-1 rounded font-mono">
                        {template.command} {template.args?.join(' ')}
                      </code>
                    )}
                    {template.url && (
                      <code className="text-xs bg-muted px-2 py-1 rounded font-mono">
                        {template.url}
                      </code>
                    )}
                  </div>
                  
                  {template.env_placeholders.length > 0 && (
                    <div className="mt-2 text-xs text-muted-foreground flex items-center gap-1">
                      <Key className="w-3 h-3" />
                      {t('requires')}: {template.env_placeholders.join(', ')}
                    </div>
                  )}
                </div>
                <ChevronRight className="w-5 h-5 text-muted-foreground" />
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
