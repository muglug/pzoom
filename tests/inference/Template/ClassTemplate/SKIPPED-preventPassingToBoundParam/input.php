<?php
/**
 * @template T
 */
class Container
{
    /** @var T */
    private $t;

    /** @param T $t */
    public function __construct($t)
    {
        $this->t = $t;
    }

    /**
     * @param T $t
     * @return T
     */
    protected function makeNew($t)
    {
        return $t;
    }

    /**
     * @template U
     * @param U $u
     */
    public function map($u) : void
    {
        $this->makeNew($u);
    }
}
