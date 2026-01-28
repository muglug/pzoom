<?php
class Foo implements BackedEnum {
    /** @psalm-pure */
    public static function cases(): array
    {
        return [];
    }

    /** @psalm-pure */
    public static function from(int|string $value): static
    {
        throw new Exception;
    }

    /** @psalm-pure */
    public static function tryFrom(int|string $value): ?static
    {
        return null;
    }
}
                
