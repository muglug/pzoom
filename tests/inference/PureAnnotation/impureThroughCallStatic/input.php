<?php
/**
 * @method static void test()
 */
final class Impure
{
    public static function __callStatic(string $name, array $arguments)
    {
    }
}

/**
 * @psalm-pure
 */
function testImpure(): void
{
    Impure::test();
}
                    
