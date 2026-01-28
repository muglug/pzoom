<?php
/**
 * @template T1
 */
interface IFoo {
    /**
     * @template T2
     * @psalm-param T2 $f
     * @psalm-return self<T2>
     */
    public static function doFoo($f): self;
}


/**
 * @template T5
 * @implements IFoo<T5>
 */
class ConcreteFooChild implements IFoo {
    /** @var T5 */
    private $baz;

    /** @param T5 $baz */
    public function __construct($baz) {
        $this->baz = $baz;
    }

    /**
     * @template T6
     * @psalm-param T6 $f
     * @psalm-return ConcreteFooChild<T6>
     */
    public static function doFoo($f): self
    {
        $r = new self($f);
        return $r;
    }
}