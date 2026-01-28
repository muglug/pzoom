<?php
/**
 * @template TP as ?scalar
 */
class Entity
{
    /**
     * @var TP
     */
    private $parent;

    /** @param TP $parent */
    public function __construct($parent) {
        $this->parent = $parent;
    }

    public function hasNoParent() : bool
    {
        return $this->parent === null; // So TP does contain null
    }
}