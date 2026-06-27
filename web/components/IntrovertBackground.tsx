import React, { useEffect, useState } from 'react';
import './IntrovertBackground.css';

interface IntrovertBackgroundProps {
  children?: React.ReactNode;
  className?: string;
}

/**
 * IntrovertBackground: A privacy-focused Web3 aesthetic background.
 * 
 * Features:
 * - Auto-detects system theme (Light/Dark).
 * - Multi-layered gradients and radial glows.
 * - Dynamic textures (Diagonal for Light, Dots for Dark).
 * - Full backdrop-filter support for overlaying cards.
 */
const IntrovertBackground: React.FC<IntrovertBackgroundProps> = ({ children, className = "" }) => {
  const [isDarkMode, setIsDarkMode] = useState(false);

  useEffect(() => {
    // Initial check
    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
    setIsDarkMode(mediaQuery.matches);

    // Listen for changes
    const handler = (e: MediaQueryListEvent) => setIsDarkMode(e.matches);
    mediaQuery.addEventListener('change', handler);
    return () => mediaQuery.removeEventListener('change', handler);
  }, []);

  return (
    <div className={`introvert-bg-container ${isDarkMode ? 'dark' : 'light'} ${className}`}>
      {/* Layer 1: Base Gradient */}
      <div className="bg-layer bg-base" />

      {/* Layer 2: Texture Patterns */}
      <div className="bg-layer bg-texture" />

      {/* Layer 3: Radial Glows */}
      <div className="glow-blob glow-indigo" />
      <div className="glow-blob glow-green" />

      {/* Content Layer (Backdrop filters apply to items inside here) */}
      <div className="introvert-content">
        {children}
      </div>
    </div>
  );
};

export default IntrovertBackground;
