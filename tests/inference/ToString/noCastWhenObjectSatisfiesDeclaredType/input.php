<?php
class A {
    public function __toString(): string
    {
        return "";
    }
}

/**
 * The returned array of `__toString` objects already satisfies the declared
 * `mixed` value type, so no implicit string cast happens and no
 * ImplicitToStringCast must be reported (matching Psalm).
 *
 * @return array<array-key, mixed>
 */
function f(A $a): array {
    return [$a];
}
