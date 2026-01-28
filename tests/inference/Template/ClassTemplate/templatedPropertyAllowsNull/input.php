<?php
/**
 * @template TKey as string|null
 */
class A {
    /** @var TKey  */
    public $key;

    /**
     * @param TKey $key
     */
    public function __construct(?string $key)
    {
        $this->key = $key;
    }
}