<?php
/**
 * @template T
 */
class InvariantFoo
{
    /**
     * @param T $value
     */
    public function set($value): void {}
}

/**
 * @template-covariant T
 * @extends InvariantFoo<T>
 */
class CovariantFoo extends InvariantFoo {}
