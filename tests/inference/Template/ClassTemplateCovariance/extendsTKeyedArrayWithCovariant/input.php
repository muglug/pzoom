<?php
/**
 * @template-covariant T1
 */
interface IParentCollection {
    /**
     * @return IParentCollection<array{0: T1}>
     */
    public function getNested(): IParentCollection;
}

/**
 * @template T2
 *
 * @extends IParentCollection<T2>
 */
interface IChildCollection extends IParentCollection {
    /**
     * @return IChildCollection<array{0: T2}>
     */
    public function getNested(): IChildCollection;
}