<?php
/**
 * @psalm-immutable
 */
class Foo {
    protected string $bar;

    public function __construct(string $bar) {
        $this->bar = $bar;
    }

    public function withBar(string $bar): self {
        $new = clone $this;
        $new->bar .= $bar;
        return $new;
    }
}
