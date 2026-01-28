<?php
/**
 * @psalm-template T
 */
interface C {
    /**
     * @psalm-param T $p
     * @psalm-return C<T>
     */
    public function filter($p) : self;
}

/**
 * @psalm-template T
 * @template-implements C<T>
 */
abstract class AC implements C {
    /**
     * @psalm-var C<T>
     */
    protected $c;

    public function filter($p) : C {
        return $this->c->filter($p);
    }
}