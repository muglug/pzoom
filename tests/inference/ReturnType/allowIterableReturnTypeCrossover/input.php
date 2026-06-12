<?php
class Foo {
    public const TYPE1 = "a";
    public const TYPE2 = "b";

    public const AVAILABLE_TYPES = [
        self::TYPE1,
        self::TYPE2,
    ];

    /**
     * @return iterable<array-key, array{foo: value-of<self::AVAILABLE_TYPES>}>
     */
    public function foo() {
        return [
            ["foo" => self::TYPE1],
            ["foo" => self::TYPE2]
        ];
    }
}
