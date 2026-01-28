<?php
namespace Ns;

class C {
    public const ARR = [
        "a" => ["foo" => true],
        "b" => []
    ];

    public function bar(?string $key): bool {
        if ($key === null || !array_key_exists($key, self::ARR) || !array_key_exists("foo", self::ARR[$key])) {
            return false;
        }

        return self::ARR[$key]["foo"];
    }
}