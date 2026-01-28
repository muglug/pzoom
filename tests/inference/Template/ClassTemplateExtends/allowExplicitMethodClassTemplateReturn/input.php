<?php
/**
 * @template T of object
 */
interface I
{
    /**
     * @return class-string<T>
     */
    public function m(): string;
}

/**
 * @template T2 of object
 * @template-implements I<T2>
 */
class C implements I
{
    /** @var T2 */
    private object $o;

    /** @param T2 $o */
    public function __construct(object $o) {
        $this->o = $o;
    }

    /**
     * @return class-string<T2>
     */
    public function m(): string {
        return get_class($this->o);
    }
}