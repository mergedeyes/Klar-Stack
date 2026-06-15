'use client';
import { Button } from '@/components/ui/button';
import { useSmartBack } from '@/hooks/use-smart-back';
import { ArrowLeft } from 'lucide-react';

export function SmartBackButton({ className, ...props }: any) {
  const goBack = useSmartBack();

  return (
    <Button 
      variant="ghost" 
      size="icon" 
      onClick={goBack} 
      className={className} 
      {...props}
    >
      <ArrowLeft size={20} />
    </Button>
  );
}