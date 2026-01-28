<?php
/**
 * @psalm-template T
 * @psalm-immutable
 */
abstract class Enum {
    /** @var T */
    private $value;

    /** @param T $value */
    public function __construct($value) {
        $this->value = $value;
    }

    /**
     * @return mixed
     * @psalm-return T
     */
    public function getValue() {
        return $this->value;
    }
}

/**
 * @extends Enum<string>
 * @psalm-immutable
 */
class TestEnum extends Enum {
    public const TEST = "test";
}

function foo(TestEnum $e): void {
    if ($e->getValue() === TestEnum::TEST
        && $e->getValue() === TestEnum::TEST
    ) {}
}
