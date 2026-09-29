'use client';
import { Button } from '@/components/ui/button';
import { useSmartBack } from '@/hooks/use-smart-back';
import { ArrowLeft } from 'lucide-react';

// onClick and children are fixed by this component, so callers can't pass them.
type SmartBackButtonProps = Omit<React.ComponentProps<typeof Button>, "onClick" | "children">;

export function SmartBackButton({ className, ...props }: SmartBackButtonProps) {
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