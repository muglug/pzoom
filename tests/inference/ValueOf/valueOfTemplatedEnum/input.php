<?php
/**
 * @template TValue of string|int
 * @psalm-require-implements BackedEnum
 */
trait ValuesFromEnumTrait
{
    /**
     * @return list<TValue>
     */
    public static function values(): array
    {
        $cases = self::cases();
        return array_map(
            static fn (BackedEnum $enum) => $enum->value,
            $cases
        );
    }
}

enum Bar: string
{
    /**
     * @use ValuesFromEnumTrait<value-of<self>>
     */
    use ValuesFromEnumTrait;

    case BAZ = "baz";
}


$values = Bar::values();
                
