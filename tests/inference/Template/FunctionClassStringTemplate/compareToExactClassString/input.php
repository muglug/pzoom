<?php
/**
 * @template T as object
 */
class Type
{
    /** @var class-string<T> */
    private $typeName;

    /**
     * @param class-string<T> $typeName
     */
    public function __construct(string $typeName) {
        $this->typeName = $typeName;
    }

    /**
     * @param mixed $value
     * @return T
     */
    public function cast($value) {
        if (is_object($value) && get_class($value) === $this->typeName) {
            return $value;
        }
        throw new RuntimeException();
    }
}