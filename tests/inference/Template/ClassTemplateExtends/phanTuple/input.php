<?php
namespace Phan\Library;

/**
 * An abstract tuple.
 */
abstract class Tuple
{
    /** @var int */
    const ARITY = 0;

    /**
     * @return int
     * The arity of this tuple
     */
    public function arity(): int
    {
        return static::ARITY;
    }

    /**
     * @return array
     * An array of all elements in this tuple.
     */
    abstract public function toArray(): array;
}

/**
 * A tuple of 1 element.
 *
 * @template T0
 * The type of element zero
 */
class Tuple1 extends Tuple
{
    /** @var int */
    const ARITY = 1;

    /** @var T0 */
    public $_0;

    /**
     * @param T0 $_0
     * The 0th element
     */
    public function __construct($_0) {
        $this->_0 = $_0;
    }

    /**
     * @return int
     * The arity of this tuple
     */
    public function arity(): int
    {
        return static::ARITY;
    }

    /**
     * @return array
     * An array of all elements in this tuple.
     */
    public function toArray(): array
    {
        return [
            $this->_0,
        ];
    }
}

/**
 * A tuple of 2 elements.
 *
 * @template T0
 * The type of element zero
 *
 * @template T1
 * The type of element one
 *
 * @extends Tuple1<T0>
 */
class Tuple2 extends Tuple1
{
    /** @var int */
    const ARITY = 2;

    /** @var T1 */
    public $_1;

    /**
     * @param T0 $_0
     * The 0th element
     *
     * @param T1 $_1
     * The 1st element
     */
    public function __construct($_0, $_1) {
        parent::__construct($_0);
        $this->_1 = $_1;
    }

    /**
     * @return array
     * An array of all elements in this tuple.
     */
    public function toArray(): array
    {
        return [
            $this->_0,
            $this->_1,
        ];
    }
}

$a = new Tuple2("cool", 5);

/** @return void */
function takes_int(int $i) {}

/** @return void */
function takes_string(string $s) {}

takes_string($a->_0);
takes_int($a->_1);
