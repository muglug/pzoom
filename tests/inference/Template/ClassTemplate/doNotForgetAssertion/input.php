<?php

class a {
    public ?int $expr = null;
}

/**
 * @template T as int
 */
class b
{
    public function test(
        a $_
    ): void {
    }
}

class c
{
    public static function analyze(
        a $container,
        b $test,
    ): int {
        $test->test($container);

        if ($container->expr) {
            if (random_int(0, 1)) {
                self::test(
                    $container,
                );
            }
            return $container->expr;
        }
        return 0;
    }

    private static function test(
        a $_,
    ): void {
    }
}
