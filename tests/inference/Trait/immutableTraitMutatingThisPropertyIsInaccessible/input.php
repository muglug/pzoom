<?php
/**
 * @psalm-immutable
 */
trait MutatingTrait {
    public function setBar(string $bar): void {
        $this->bar = $bar;
    }
}

/**
 * @psalm-immutable
 */
final class Foo {
    use MutatingTrait;

    public function __construct(public string $bar) {}
}
