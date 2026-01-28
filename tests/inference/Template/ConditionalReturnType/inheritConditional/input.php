<?php
/**
 * @template E
 */
interface AInterface {
    /**
     * @template T
     * @template T2 as "int"|class-string<T>
     * @param T2 $type
     * @return (T2 is "int" ? static<int> : static<T>)
     */
    public static function ofType(string $type);
}


/**
 * @template E
 * @implements AInterface<E>
 * @psalm-consistent-constructor
 * @psalm-consistent-templates
 */
class BClass implements AInterface {
    protected string $type;

    protected function __construct(string $type)
    {
        $this->type   = $type;
    }

    /**
     * @template T
     * @template T2 as "int"|class-string<T>
     * @param T2 $type
     * @return (T2 is "int" ? static<int> : static<T>)
     */
    public static function ofType(string $type) {
        return new static($type);
    }
}