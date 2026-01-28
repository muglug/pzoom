<?php
/**
 * @param pure-callable(string,string):int $_p
 */
function f(callable $_p): void {}

class C {
    /**
     * @psalm-pure
     */
    public static function m(string $a, string $b): int { return $a <=> $b; }
}

f("strcmp");
f([new C, "m"]);
f([C::class, "m"]);
