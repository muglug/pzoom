<?php
/**
 * @template T as object
 */
class Base {
    /** @param T $_o */
    public function __construct($_o) {}
    /**
     * @return T
     * @psalm-suppress InvalidReturnType
     */
    public function t() {}
}

/** @template-extends Base<int> */
class SpecializedByInheritance extends Base {}
