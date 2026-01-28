<?php
namespace Ns;

class C {
    public const ARR = [
        "a" => ["foo" => true],
        "b" => [],
    ];
}

function bar(?string $key): bool {
    if ($key === null || !array_key_exists($key, C::ARR) || !array_key_exists("foo", C::ARR[$key])) {
        return false;
    }

    return C::ARR[$key]["foo"];
}