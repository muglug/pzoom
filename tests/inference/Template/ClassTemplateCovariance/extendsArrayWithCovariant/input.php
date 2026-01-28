<?php
/**
 * @template-covariant T1
 */
interface IParentCollection {
    /**
     * @return IParentCollection<array<T1>>
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
     * @return IChildCollection<array<T2>>
     */
    public function getNested(): IChildCollection;
}