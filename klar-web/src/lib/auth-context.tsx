"use client";

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useState,
} from "react";
import { auth, tokens, users, type User } from "@/lib/api";

// ── Types ─────────────────────────────────────────────────────────────────────

interface AuthContextValue {
  user: User | null;
  loading: boolean;
  login: (email: string, password: string) => Promise<void>;
  register: (
    username: string,
    email: string,
    password: string
  ) => Promise<void>;
  logout: () => Promise<void>;
}

// ── Context ───────────────────────────────────────────────────────────────────

const AuthContext = createContext<AuthContextValue | null>(null);

export function AuthProvider({ children }: { children: React.ReactNode }) {
  const [user, setUser] = useState<User | null>(null);
  const [loading, setLoading] = useState(true);

  // On mount — if we have a stored token, fetch the current user
  useEffect(() => {
    const restore = async () => {
      if (!tokens.getAccess()) {
        setLoading(false);
        return;
      }
      try {
        const me = await users.me();
        setUser(me);
      } catch {
        // Token expired or invalid — try refresh
        const refreshToken = tokens.getRefresh();
        if (refreshToken) {
          try {
            const refreshed = await auth.refresh(refreshToken);
            tokens.set(refreshed.access_token, refreshed.refresh_token);
            const me = await users.me();
            setUser(me);
          } catch {
            tokens.clear();
          }
        } else {
          tokens.clear();
        }
      } finally {
        setLoading(false);
      }
    };

    restore();
  }, []);

  const login = useCallback(async (email: string, password: string) => {
    const res = await auth.login(email, password);
    tokens.set(res.access_token, res.refresh_token);
    setUser(res.user);
  }, []);

  const register = useCallback(
    async (username: string, email: string, password: string) => {
      const res = await auth.register(username, email, password);
      tokens.set(res.access_token, res.refresh_token);
      setUser(res.user);
    },
    []
  );

  const logout = useCallback(async () => {
    const refreshToken = tokens.getRefresh();
    if (refreshToken) {
      try {
        await auth.logout(refreshToken);
      } catch {
        // Best-effort — clear locally regardless
      }
    }
    tokens.clear();
    setUser(null);
  }, []);

  return (
    <AuthContext.Provider value={{ user, loading, login, register, logout }}>
      {children}
    </AuthContext.Provider>
  );
}

// ── Hook ──────────────────────────────────────────────────────────────────────

export function useAuth() {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error("useAuth must be used inside <AuthProvider>");
  return ctx;
}
