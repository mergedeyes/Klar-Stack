'use client';
import { useRouter } from 'next/navigation';

export function useSmartBack(fallback = '/feed') {
  const router = useRouter();

  return () => {
    if (window.history.length > 1) {
      router.back();
    } else {
      router.push(fallback);
    }
  };
}