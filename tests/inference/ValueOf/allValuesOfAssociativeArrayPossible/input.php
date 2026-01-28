<?php
class A {
    const FOO = [
        "bar" => 42,
        "adams" => 43,
    ];
    /** @return value-of<A::FOO> */
    public function getValue(bool $adams) {
        if ($adams) {
            return 42;
        }
        return 43;
    }
}
