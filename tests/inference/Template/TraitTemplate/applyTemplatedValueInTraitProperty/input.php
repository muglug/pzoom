<?php
/** @template T */
trait ValueTrait {
    /** @psalm-param T $value */
    public function setValue($value): void {
        $this->value = $value;
    }
}

class C {
    /** @use ValueTrait<string> */
    use ValueTrait;

    /** @var string */
    private $value;

    public function __construct(string $value) {
        $this->value = $value;
    }
}