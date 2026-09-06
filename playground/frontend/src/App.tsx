import React, { useState, useEffect } from 'react';
import { Header, PageTab } from './components/Header';
import { Footer } from './components/Footer';
import { HomePage } from './pages/HomePage';
import { UseCasesPage } from './pages/UseCasesPage';
import { ArchitecturePage } from './pages/ArchitecturePage';
import { EconomicsPage } from './pages/EconomicsPage';
import { DocsPage } from './pages/DocsPage';
import { PricingPage } from './pages/PricingPage';
import { PlaygroundWorkspace } from './components/PlaygroundWorkspace';
import { useSessions } from './hooks/useSessions';

export function App() {
  const [currentTab, setCurrentTab] = useState<PageTab>('overview');
  const { sessions } = useSessions();

  // Read initial route from URL path or hash
  useEffect(() => {
    const parseRoute = () => {
      const path = window.location.pathname.replace(/^\/playground\/?/, 'playground').replace(/^\//, '');
      const hash = window.location.hash.replace(/^#\/?/, '');
      const target = hash || path;

      if (target === 'use-cases') setCurrentTab('use-cases');
      else if (target === 'architecture') setCurrentTab('architecture');
      else if (target === 'economics') setCurrentTab('economics');
      else if (target === 'docs') setCurrentTab('docs');
      else if (target === 'pricing') setCurrentTab('pricing');
      else if (target === 'playground' || target.startsWith('playground')) setCurrentTab('playground');
      else setCurrentTab('overview');
    };

    parseRoute();
    window.addEventListener('popstate', parseRoute);
    window.addEventListener('hashchange', parseRoute);
    return () => {
      window.removeEventListener('popstate', parseRoute);
      window.removeEventListener('hashchange', parseRoute);
    };
  }, []);

  const handleSelectTab = (tab: PageTab) => {
    setCurrentTab(tab);
    const newPath = tab === 'overview' ? '/' : `/${tab}`;
    window.history.pushState({ tab }, '', newPath);
    window.scrollTo({ top: 0, behavior: 'smooth' });
  };

  return (
    <div className={`bg-bg text-text flex flex-col font-sans selection:bg-accent selection:text-white ${
      currentTab === 'playground' ? 'h-screen overflow-hidden' : 'min-h-screen'
    }`}>
      {/* Universal TE Top Navigation Header */}
      <Header
        currentTab={currentTab}
        onSelectTab={handleSelectTab}
        activeSessionCount={sessions.length}
      />

      {/* Main Viewport Container */}
      <main className={`flex-1 flex flex-col ${currentTab === 'playground' ? 'min-h-0 overflow-hidden' : ''}`}>
        {currentTab === 'overview' && <HomePage onNavigate={handleSelectTab} />}
        {currentTab === 'use-cases' && <UseCasesPage onNavigate={handleSelectTab} />}
        {currentTab === 'architecture' && <ArchitecturePage onNavigate={handleSelectTab} />}
        {currentTab === 'economics' && <EconomicsPage onNavigate={handleSelectTab} />}
        {currentTab === 'docs' && <DocsPage onNavigate={handleSelectTab} />}
        {currentTab === 'pricing' && <PricingPage onNavigate={handleSelectTab} />}
        {currentTab === 'playground' && <PlaygroundWorkspace />}
      </main>

      {/* Technical Footer (Only rendered on marketing/documentation pages) */}
      {currentTab !== 'playground' && <Footer onSelectTab={handleSelectTab} />}
    </div>
  );
}

export default App;
