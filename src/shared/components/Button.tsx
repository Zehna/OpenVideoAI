import React from 'react';

export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'primary' | 'secondary' | 'danger';
  isLoading?: boolean;
}

export const Button: React.FC<ButtonProps> = ({
  children,
  variant = 'primary',
  isLoading = false,
  disabled,
  className = '',
  ...props
}) => {
  const baseStyles =
    'px-4 py-2 rounded-lg font-medium text-sm transition-colors focus:outline-none focus:ring-2 focus:ring-offset-2 disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer inline-flex items-center justify-center';

  const variantStyles = {
    primary:
      'bg-indigo-600 text-white hover:bg-indigo-500 focus:ring-indigo-400 focus:ring-offset-gray-900',
    secondary:
      'bg-gray-700 text-gray-100 hover:bg-gray-600 focus:ring-gray-400 focus:ring-offset-gray-900',
    danger:
      'bg-rose-600 text-white hover:bg-rose-500 focus:ring-rose-400 focus:ring-offset-gray-900',
  }[variant];

  return (
    <button
      className={`${baseStyles} ${variantStyles} ${className}`}
      disabled={disabled || isLoading}
      {...props}
    >
      {isLoading ? 'Loading...' : children}
    </button>
  );
};
