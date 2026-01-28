<?php
/**
 * @template TReal
 */
interface Collection
{
    /**
     * @return array<TReal>
     */
    function toArray();
}

/**
 * @template TDummy
 * @implements Collection<int>
 */
class IntCollection implements Collection
{
    /** @param TDummy $t */
    public function __construct($t) {

    }

    public function toArray() {
        return ["foo"];
    }
}
