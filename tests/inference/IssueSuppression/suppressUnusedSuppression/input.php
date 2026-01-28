<?php
class Foo {
    /**
     * @psalm-suppress UnusedPsalmSuppress, MissingPropertyType
     */
    public string $bar = "baz";

    /**
     * @psalm-suppress UnusedPsalmSuppress, MissingReturnType
     */
    public function foobar(): string
    {
        return "foobar";
    }
}
                
