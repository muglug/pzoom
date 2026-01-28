<?php
class A {
    const FOO = [
        "bar" => 42,
        "adams" => 43,
    ];
    /** @return key-of<A::FOO> */
    public function getKey(bool $adams) {
        if ($adams) {
            return "adams";
        }
        return "bar";
    }
}
