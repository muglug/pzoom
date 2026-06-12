<?php
/** @psalm-type OpeningTypes=self::TYPE_A|self::TYPE_B */
class Foo {
    public const TYPE_A = 1;
    public const TYPE_B = 2;
}

/**
 * @psalm-import-type OpeningTypes from Foo
 * @psalm-type OpeningTypeAssignment=list<OpeningTypes>
 */
class Main {
    /** @return OpeningTypeAssignment */
    public function doStuff(): array {
        return [];
    }
}

$instance = new Main();
$output = $instance->doStuff();
