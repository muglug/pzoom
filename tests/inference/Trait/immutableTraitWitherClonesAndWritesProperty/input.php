<?php
/**
 * @psalm-immutable
 */
trait WitherTrait {
    public function withBar(string $bar): static {
        $clone = clone $this;
        $clone->bar = $bar;
        return $clone;
    }
}

/**
 * @psalm-immutable
 */
final class Foo {
    use WitherTrait;

    public function __construct(public string $bar) {}
}
