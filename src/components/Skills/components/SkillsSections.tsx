import { Download, FolderPlus, Loader2, RefreshCw, Sparkles, Trash2 } from 'lucide-react';
import { allModels, formatTs, modelIconMap, modelTabs } from '../helpers';
import type { SkillsView } from '../view';

export function SkillsHeader({ view }: { view: SkillsView }) {
  const { t, activeMode, message, loading, refreshingSources, handleSyncSources, handleImportRepositoryFolder } = view;
  return (
    <div className="flex items-center justify-between">
      <div>
        <h2 className="text-xl font-bold tracking-tight">{t('skillsPageTitle', 'Global Skills')}</h2>
        <p className="text-sm text-muted-foreground">
          {t(
            'skillsPageDesc',
            'Install and manage skills at global scope by model. These installs are not tied to a single workspace. Use workspace pages for project-specific installs.'
          )}
        </p>
      </div>
      <div className="flex items-center gap-2">
        {message && (
          <div
            className={`text-xs rounded-md border px-2.5 py-1.5 ${
              message.type === 'error'
                ? 'bg-destructive/10 text-destructive border-destructive/20'
                : 'bg-green-500/10 text-green-700 border-green-500/20'
            }`}
          >
            {message.text}
          </div>
        )}
        {activeMode === 'recommended' && (
          <button
            onClick={handleSyncSources}
            disabled={loading || refreshingSources}
            className="px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm inline-flex items-center gap-2 disabled:opacity-50"
          >
            <RefreshCw className={`w-4 h-4 ${loading || refreshingSources ? 'animate-spin' : ''}`} />
            {t('skillsSyncSources', '同步源列表')}
          </button>
        )}
        {activeMode === 'repository' && (
          <button
            onClick={handleImportRepositoryFolder}
            disabled={loading}
            className="px-4 py-2 border rounded-md text-sm font-medium inline-flex items-center gap-2 hover:bg-muted disabled:opacity-50"
          >
            <FolderPlus className="w-4 h-4" />
            {t('skillsLocalImportButton', 'Import From Folder')}
          </button>
        )}
      </div>
    </div>
  );
}

export function SkillsModeTabs({ view }: { view: SkillsView }) {
  const { t, activeMode, setActiveMode, handleSwitchToRecommended, handleSwitchToRepository } = view;
  return (
    <div className="inline-flex w-fit rounded-lg border border-black bg-white p-1">
      <button
        onClick={handleSwitchToRecommended}
        className={`px-3 py-1.5 rounded-md text-sm ${
          activeMode === 'recommended' ? 'bg-black text-white' : 'bg-white text-black'
        }`}
      >
        {t('recommended', '推荐')}
      </button>
      <button
        onClick={handleSwitchToRepository}
        className={`px-3 py-1.5 rounded-md text-sm ${
          activeMode === 'repository' ? 'bg-black text-white' : 'bg-white text-black'
        }`}
      >
        {t('repository', '仓库')}
      </button>
      <button
        onClick={() => setActiveMode('installed')}
        className={`px-3 py-1.5 rounded-md text-sm ${
          activeMode === 'installed' ? 'bg-black text-white' : 'bg-white text-black'
        }`}
      >
        {t('installed', '已安装')}
      </button>
    </div>
  );
}

export function SkillsModelTabs({ view }: { view: SkillsView }) {
  const { t, activeMode, activeModel, setActiveModel, recommendedCounts, installedCounts } = view;
  return (
    <div className="border rounded-xl bg-card p-3">
      <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
        {modelTabs.map((m) => {
          const ModelIcon = modelIconMap[m.id];
          return (
            <button
              key={m.id}
              type="button"
              onClick={() => setActiveModel(m.id)}
              className={`rounded-lg border px-4 py-3 text-left transition-all ${
                activeModel === m.id ? 'border-primary bg-primary/5' : 'hover:bg-muted/40 hover:-translate-y-0.5'
              }`}
            >
              <div className="flex items-center gap-2">
                <ModelIcon className="w-5 h-5" />
                <span className="text-sm font-semibold">{m.label}</span>
              </div>
              <div className="mt-2.5">
                <span className="text-sm leading-none text-muted-foreground">
                  {activeMode === 'recommended'
                    ? t('skillsRecommendedCount', 'Recommended {{count}} skills', { count: recommendedCounts[m.id] ?? 0 })
                    : t('skillsInstalledCount', 'Installed {{count}} skills', { count: installedCounts[m.id] ?? 0 })}
                </span>
              </div>
            </button>
          );
        })}
      </div>
    </div>
  );
}

export function SkillsInstalledSection({ view }: { view: SkillsView }) {
  const {
    t,
    initialLoadDone,
    visibleInstalled,
    handleSwitchToRecommended,
    pickIcon,
    reinstallingKeys,
    handleOpenDetail,
    handleReinstall,
    handleUninstall,
  } = view;
  return (
    <>
      {!initialLoadDone ? (
        <div className="text-center py-12 text-muted-foreground">
          <Loader2 className="w-8 h-8 mx-auto mb-3 animate-spin" />
          <p>{t('loading', 'Loading...')}</p>
        </div>
      ) : visibleInstalled.length === 0 ? (
        <div className="text-center py-12">
          <Sparkles className="w-16 h-16 mx-auto text-muted-foreground mb-4" />
          <h3 className="text-lg font-semibold mb-2">{t('noInstalledSkillsForModel', '该模型下暂无已安装 Skills')}</h3>
          <p className="text-muted-foreground">{t('noInstalledSkillsForModelDesc', '你可以先到“推荐”中安装 Skills。')}</p>
          <button
            onClick={handleSwitchToRecommended}
            className="mt-4 px-4 py-2 bg-primary text-primary-foreground rounded-md text-sm"
          >
            {t('recommended', '推荐')}
          </button>
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3">
          {visibleInstalled.map((skill) => {
            const Icon = pickIcon(skill.icon_seed || skill.id);
            const reinstallKey = `${skill.model}:${skill.id}`;
            const reinstalling = !!reinstallingKeys[reinstallKey];
            return (
              <div
                key={`${skill.model}:${skill.id}`}
                className="border rounded-xl p-4 bg-card transition-all duration-200 hover:-translate-y-0.5 hover:shadow-md hover:border-primary/30 cursor-pointer"
                onClick={() => handleOpenDetail(skill)}
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="p-2 rounded-md bg-primary/10 text-primary">
                    <Icon className="w-4 h-4" />
                  </div>
                  <span className="text-[10px] text-muted-foreground line-clamp-1 max-w-[11rem] text-right">
                    {skill.dir_name || skill.source_rel_path.split('/').pop() || skill.id}
                  </span>
                </div>

                <h4 className="mt-3 font-semibold text-sm line-clamp-1">{skill.name}</h4>
                <p className="mt-1 text-xs text-muted-foreground line-clamp-2">{skill.description}</p>

                <div className="mt-3 text-[11px] text-muted-foreground">
                  {t('lastUpdated', 'Last updated')}: {formatTs(skill.updated_at || skill.installed_at)}
                </div>

                <div className="mt-3 flex items-center justify-end gap-2">
                  <button
                    disabled={reinstalling}
                    className="text-xs px-2.5 py-1 rounded-md border hover:bg-muted inline-flex items-center gap-1 disabled:opacity-50 disabled:cursor-not-allowed"
                    onClick={(e) => {
                      e.stopPropagation();
                      handleReinstall(skill);
                    }}
                  >
                    <RefreshCw className={`w-3.5 h-3.5 ${reinstalling ? 'animate-spin' : ''}`} />
                    {t('skillsReinstall', '重新安装')}
                  </button>
                  <button
                    className="text-xs px-2.5 py-1 rounded-md border hover:bg-destructive/10 text-destructive inline-flex items-center gap-1"
                    onClick={(e) => {
                      e.stopPropagation();
                      handleUninstall(skill);
                    }}
                  >
                    <Trash2 className="w-3.5 h-3.5" />
                    {t('uninstall', 'Uninstall')}
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </>
  );
}

export function SkillsRepositorySection({ view }: { view: SkillsView }) {
  const {
    t,
    repositorySearch,
    setRepositorySearch,
    repositorySourceFilter,
    setRepositorySourceFilter,
    initialLoadDone,
    visibleRepository,
    pickIcon,
    getRepoSourceMeta,
    isRecentRepositorySkill,
    handleOpenRepositoryDetail,
    openReloadPreviewByRepoKey,
    handleInstallRepository,
    handleDeleteRepository,
  } = view;
  return (
    <>
      <div className="mb-4 flex items-center gap-2">
        <div className="w-[170px] sm:w-[220px] lg:w-[260px] shrink-0">
          <input
            value={repositorySearch}
            onChange={(e) => setRepositorySearch(e.target.value)}
            placeholder={t('skillsSearchPlaceholder', '搜索 Skill 名称或描述')}
            className="h-9 w-full rounded-lg border border-black/20 bg-white px-3 text-sm shadow-sm outline-none focus:border-black"
          />
        </div>
        <div className="min-w-0 flex-1 overflow-x-auto">
          <div className="flex w-max min-w-full justify-end">
            <div className="inline-flex w-max rounded-lg border border-black bg-white p-1 whitespace-nowrap shadow-sm">
              <button
                onClick={() => setRepositorySourceFilter('all')}
                className={`shrink-0 px-3 py-1.5 rounded-md text-sm ${
                  repositorySourceFilter === 'all'
                    ? 'bg-black text-white'
                    : 'bg-white text-black'
                }`}
              >
                {t('all', '全部')}
              </button>
              <button
                onClick={() => setRepositorySourceFilter('local')}
                className={`shrink-0 px-3 py-1.5 rounded-md text-sm ${
                  repositorySourceFilter === 'local'
                    ? 'bg-black text-white'
                    : 'bg-white text-black'
                }`}
              >
                {t('skillsSourceTypeLocalImport', '本地导入')}
              </button>
              <button
                onClick={() => setRepositorySourceFilter('remote')}
                className={`shrink-0 px-3 py-1.5 rounded-md text-sm ${
                  repositorySourceFilter === 'remote'
                    ? 'bg-black text-white'
                    : 'bg-white text-black'
                }`}
              >
                {t('skillsSourceTypeRemote', '推荐源')}
              </button>
            </div>
          </div>
        </div>
      </div>

      {!initialLoadDone ? (
        <div className="text-center py-12 text-muted-foreground">
          <Loader2 className="w-8 h-8 mx-auto mb-3 animate-spin" />
          <p>{t('loading', 'Loading...')}</p>
        </div>
      ) : visibleRepository.length === 0 ? (
        <div className="text-center py-12">
          <Sparkles className="w-16 h-16 mx-auto text-muted-foreground mb-4" />
          <h3 className="text-lg font-semibold mb-2">{t('noResultsFound', 'No skills found.')}</h3>
          <p className="text-muted-foreground mb-4">
            {t('skillsRepoEmptyHint', '请从文件夹导入 Skill，或先在推荐模式同步源列表。')}
          </p>
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3">
          {visibleRepository.map((repo) => {
            const Icon = pickIcon(repo.icon_seed || repo.skill_id);
            const sourceMeta = getRepoSourceMeta(repo.source_type);
            const installedCount = allModels.reduce(
              (sum, model) => sum + (repo.installed[model] ? 1 : 0),
              0,
            );
            const repoHasUpdate = !!repo.has_update;
            const isNewRepo = isRecentRepositorySkill(repo);
            return (
              <div
                key={repo.repo_key}
                className="border rounded-xl p-4 bg-card transition-all duration-200 hover:-translate-y-0.5 hover:shadow-md hover:border-primary/30 cursor-pointer"
                onClick={() => handleOpenRepositoryDetail(repo)}
              >
                <div className="flex items-start justify-between">
                  <div className="p-2 rounded-md bg-muted text-foreground">
                    <Icon className="w-4 h-4" />
                  </div>
                  <div className="flex flex-col items-end gap-1.5">
                    <span className="text-[10px] text-muted-foreground line-clamp-1 max-w-[11rem] text-right">
                      {repo.dir_name || repo.source_rel_path.split('/').pop() || repo.skill_id}
                    </span>
                    <div className="flex items-center gap-1.5">
                      {isNewRepo && (
                        <span className="text-[10px] px-1.5 py-0.5 rounded border bg-emerald-500/10 text-emerald-700 border-emerald-500/30">
                          {t('new', 'New')}
                        </span>
                      )}
                      {repoHasUpdate && (
                        <button
                          type="button"
                          className="text-[10px] px-2 py-0.5 rounded-full bg-amber-100 text-amber-700 border border-amber-200"
                          onClick={(e) => {
                            e.stopPropagation();
                            void openReloadPreviewByRepoKey(repo.repo_key);
                          }}
                        >
                          {t('hasUpdate', '有更新')}
                        </button>
                      )}
                    </div>
                  </div>
                </div>

                <h4 className="mt-3 font-semibold text-sm line-clamp-1">{repo.name}</h4>
                <p className="mt-1 text-xs text-muted-foreground line-clamp-2">{repo.description}</p>
                <div className="mt-3 text-[11px] text-muted-foreground flex items-center gap-4">
                  <span>
                    {t('skillsRepositoryLastUpdated', '最后更新')}: {formatTs(repo.updated_at || repo.created_at)}
                  </span>
                  <span>
                    {t('installed', 'Installed')} {installedCount}/4
                  </span>
                </div>

                <div className="mt-3 flex items-center justify-between gap-2">
                  <span className={`text-[10px] px-2 py-1 rounded border ${sourceMeta.className}`}>
                    {sourceMeta.label}
                  </span>
                  <div className="flex justify-end gap-2">
                    {repo.models.length > 0 && (
                      <button
                        className="text-xs px-2.5 py-1 rounded-md bg-primary text-primary-foreground inline-flex items-center gap-1"
                        onClick={(e) => {
                          e.stopPropagation();
                          handleInstallRepository(repo);
                        }}
                      >
                        <Download className="w-3.5 h-3.5" />
                        {t('install', 'Install')}
                      </button>
                    )}
                    <button
                      className="text-xs px-2.5 py-1 rounded-md border hover:bg-destructive/10 text-destructive inline-flex items-center gap-1"
                      onClick={(e) => {
                        e.stopPropagation();
                        handleDeleteRepository(repo);
                      }}
                    >
                      <Trash2 className="w-3.5 h-3.5" />
                      {t('delete', 'Delete')}
                    </button>
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </>
  );
}

export function SkillsRecommendedSection({ view }: { view: SkillsView }) {
  const {
    t,
    catalogSources,
    recommendedSearch,
    setRecommendedSearch,
    recommendedSourceFilter,
    setRecommendedSourceFilter,
    initialLoadDone,
    visibleCatalog,
    installedById,
    pickIcon,
    sourceStatusMap,
    syncState,
    isRecentCatalogSkill,
    handleOpenCatalogDetail,
    handleInstall,
  } = view;
  return (
    <div className="relative isolate">
      {catalogSources.length > 0 && (
        <div className="sticky top-0 z-[90] mb-4 pointer-events-none">
          <div className="pointer-events-auto relative z-[100] flex items-center gap-2">
            <div className="w-[170px] sm:w-[220px] lg:w-[260px] shrink-0">
              <input
                value={recommendedSearch}
                onChange={(e) => setRecommendedSearch(e.target.value)}
                placeholder={t('skillsSearchPlaceholder', '搜索 Skill 名称或描述')}
                className="h-9 w-full rounded-lg border border-black/20 bg-white px-3 text-sm shadow-sm outline-none focus:border-black"
              />
            </div>
            <div className="min-w-0 flex-1 overflow-x-auto">
              <div className="flex w-max min-w-full justify-end">
                <div className="inline-flex w-max rounded-lg border border-black bg-white p-1 whitespace-nowrap shadow-sm">
                  <button
                    onClick={() => setRecommendedSourceFilter('all')}
                    className={`shrink-0 px-3 py-1.5 rounded-md text-sm ${
                      recommendedSourceFilter === 'all' ? 'bg-black text-white' : 'bg-white text-black'
                    }`}
                  >
                    {t('all', '全部')}
                  </button>
                  {catalogSources.map((source) => (
                    <button
                      key={source.id}
                      title={source.id}
                      onClick={() => setRecommendedSourceFilter(source.id)}
                      className={`shrink-0 px-3 py-1.5 rounded-md text-sm ${
                        recommendedSourceFilter === source.id ? 'bg-black text-white' : 'bg-white text-black'
                      }`}
                    >
                      {source.label}
                    </button>
                  ))}
                </div>
              </div>
            </div>
          </div>
        </div>
      )}
      {!initialLoadDone ? (
        <div className="text-center py-12 text-muted-foreground">
          <Loader2 className="w-8 h-8 mx-auto mb-3 animate-spin" />
          <p>{t('loading', 'Loading...')}</p>
        </div>
      ) : visibleCatalog.length === 0 ? (
        <div className="text-center py-12">
          <Sparkles className="w-16 h-16 mx-auto text-muted-foreground mb-4" />
          <h3 className="text-lg font-semibold mb-2">{t('noRecommendedSkills', '当前没有可推荐的 Skills')}</h3>
          <p className="text-muted-foreground mb-4">{t('noRecommendedSkillsDesc', '请检查 Skills 源配置，或同步源列表后重试。')}</p>
        </div>
      ) : (
        <div className="relative z-0 grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-3">
          {visibleCatalog.map((item) => {
            const installedSkill = installedById.get(`${item.source_id}:${item.rel_path}`);
            const Icon = pickIcon(item.id);
            const srcStatus = sourceStatusMap.get(item.source_id);
            const isNewSkill = isRecentCatalogSkill(item);
            return (
              <div
                key={`${item.source_id}:${item.id}`}
                className="relative z-0 border rounded-xl p-4 bg-card transition-all duration-200 hover:shadow-md hover:border-primary/30 cursor-pointer"
                onClick={() => handleOpenCatalogDetail(item)}
              >
                <div className="flex items-start justify-between">
                  <div className="p-2 rounded-md bg-muted text-foreground">
                    <Icon className="w-4 h-4" />
                  </div>
                  <div className="flex flex-col items-end gap-1">
                    <span className="text-[10px] text-muted-foreground line-clamp-1 max-w-[11rem]">
                      {item.dir_name || item.rel_path.split('/').pop() || item.id}
                    </span>
                  </div>
                </div>
                <h4 className="mt-3 font-semibold text-sm line-clamp-1">{item.name}</h4>
                <p className="mt-1 text-xs text-muted-foreground line-clamp-2">{item.description}</p>
                <div className="mt-3 text-[11px] text-muted-foreground">
                  {t('lastSynced', 'Last synced')}: {formatTs(srcStatus?.last_synced_at || syncState?.last_sync_at)}
                </div>

                <div className="mt-3 flex items-center justify-between gap-2">
                  <div className="flex items-center gap-2">
                    <span className="text-[10px] px-2 py-1 rounded border bg-muted/50 text-muted-foreground">
                      {item.source_id}
                    </span>
                    {isNewSkill && (
                      <span className="text-[10px] px-1.5 py-0.5 rounded border bg-emerald-500/10 text-emerald-700 border-emerald-500/30">
                        {t('new', 'New')}
                      </span>
                    )}
                  </div>
                  <div className="flex justify-end">
                    {installedSkill ? (
                      <span className="text-xs px-2.5 py-1 rounded-md border text-muted-foreground inline-flex items-center gap-1">
                        <Download className="w-3.5 h-3.5" />
                        {t('installed', 'Installed')}
                      </span>
                    ) : (
                      <button
                        className="text-xs px-2.5 py-1 rounded-md bg-primary text-primary-foreground inline-flex items-center gap-1"
                        onClick={(e) => {
                          e.stopPropagation();
                          handleInstall(item);
                        }}
                      >
                        <Download className="w-3.5 h-3.5" />
                        {t('install', 'Install')}
                      </button>
                    )}
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
