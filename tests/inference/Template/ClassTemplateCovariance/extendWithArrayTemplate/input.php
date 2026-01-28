<?php
/**
 * @template-covariant T1
 */
interface C {
    /**
     * @psalm-return self<array<int, T1>>
     */
    public function zip();
}

/**
 * @template T2
 * @extends C<T2>
 */
interface AC extends C {
    /**
     * @psalm-return self<array<int, T2>>
     */
    public function zip();
}