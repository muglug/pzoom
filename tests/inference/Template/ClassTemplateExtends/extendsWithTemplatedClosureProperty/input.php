<?php
/**
 * @template T1
 */
class A
{
    /**
     * @var T1|null
     */
    protected $type;

    /**
     * @var (Closure(): T1)|null
     */
    protected $closure;
}

/**
 * @template T2
 * @extends A<T2>
 */
class B extends A {
    /**
     * @return T2|null
     */
    public function getType() {
        return $this->type;
    }

    /**
     * @return (Closure(): T2)|null
     */
    public function getClosureReturningType() {
        return $this->closure;
    }
}