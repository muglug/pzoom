<?php

class MockObject {
    public function check(string $s): void {}
}

interface Hook {
    public static function go(string $function_id): void;
}

function makeHook(MockObject $m): Hook
{
    return new class ($m) implements Hook
    {
        private static MockObject $m;

        public function __construct(MockObject $m)
        {
            self::$m = $m;
        }

        public static function go(string $function_id): void
        {
            self::$m->check($function_id);
        }
    };
}
