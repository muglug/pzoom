<?php
/**
 * @template TKey as array-key
 * @template TValue
 */
trait T1 {
    /**
     * @var array<TKey, TValue>
     */
    protected $mocks = [];

    /**
     * @param TKey $offset
     * @return TValue|null
     * @psalm-suppress LessSpecificImplementedReturnType
     * @psalm-suppress ImplementedParamTypeMismatch
     */
    public function offsetGet($offset) {
        return $this->mocks[$offset] ?? null;
    }
}

/**
 * @template TKey as array-key
 * @template TValue
 */
interface Arr {
    /**
     * @param TKey $offset
     * @return TValue|null
     */
    public function offsetGet($offset);
}

/**
 * @template TKey as array-key
 * @template TValue
 * @implements Arr<TKey, TValue>
 */
class C implements Arr {
    /** @use T1<TKey, TValue> */
    use T1;
}

/**
 * @psalm-suppress MissingTemplateParam
 */
class D extends C {
    /**
     * @param mixed $offset
     * @psalm-suppress MixedArgument
     */
    public function foo($offset) : void {
        $this->offsetGet($offset)->bar();
    }
}
