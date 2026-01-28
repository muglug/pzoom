<?php
class A {
    /** @var array */
    const FOO = [
        "bar" => 42,
        "adams" => 43,
    ];
    /** @return value-of<self::FOO>[] */
    public function getValues() {
        return array_values(self::FOO);
    }
}
