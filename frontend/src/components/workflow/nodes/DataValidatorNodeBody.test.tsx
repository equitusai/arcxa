import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { DataValidatorNodeBody } from './DataValidatorNodeBody';

describe('DataValidatorNodeBody', () => {
  it('renders object-shaped backend rule types without throwing', () => {
    render(
      <DataValidatorNodeBody
        status="idle"
        config={{
          fail_on_error: true,
          rules: [
            {
              field: 'EMAIL',
              rule_type: {
                REGEX: {
                  pattern: '^[^@\\s]+@[^@\\s]+\\.[^@\\s]+$',
                },
              } as never,
              severity: 'error',
            },
          ],
        }}
      />
    );

    expect(screen.getByText('EMAIL')).toBeTruthy();
    expect(screen.getByText(/REGEX/)).toBeTruthy();
  });
});
