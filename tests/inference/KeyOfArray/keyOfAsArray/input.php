<?php
class A {
    /** @var array */
    const FOO = [
        "bar" => 42,
        "adams" => 43,
    ];
    /** @return key-of<self::FOO>[] */
    public function getKey() {
        return array_keys(self::FOO);
    }
}
