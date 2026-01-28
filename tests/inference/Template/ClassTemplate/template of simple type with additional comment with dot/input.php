<?php
/**
 * @psalm-template T of string
 *
 * lorem ipsum.
 */
class Foo {
    /** @psalm-var T */
    public string $t;

    /** @psalm-param T $t */
    public function __construct(string $t) {
        $this->t = $t;
    }

    /**
     * @psalm-return T
     */
    public function t(): string {
        return $this->t;
    }
}
$t = (new Foo(''))->t();
                
