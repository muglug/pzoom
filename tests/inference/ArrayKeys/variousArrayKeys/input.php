<?php
/**
 * @psalm-type TAlias = 123
 */
class a {}

/**
 * @psalm-import-type TAlias from a
 * @template TKey as array-key
 * @template TValue as array-key
 * @template T as array<TKey, TValue>
 *
 * @template TOrig as a|b
 * @template TT as class-string<TOrig>
 *
 * @template TBool as bool
 */
class b {
    /**
     * @var array<TAlias, int>
     */
    private array $a = [123 => 123];

    /** @var array<value-of<T>, int> */
    public array $c = [];

    /** @var array<key-of<T>, int> */
    public array $d = [];

    /** @var array<TT, int> */
    public array $e = [];

    /** @var array<key-of<array<int, string>>, int> */
    private array $f = [123 => 123];

    /** @var array<value-of<array<int, string>>, int> */
    private array $g = ["test" => 123];

    /** @var array<TBool is true ? string : int, int> */
    private array $h = [123 => 123];

    /**
     * @return array<$v is true ? "a" : 123, 123>
     */
    public function test(bool $v): array {
        return $v ? ["a" => 123] : [123 => 123];
    }
}

/** @var b<"testKey", "testValue", array<"testKey", "testValue">, b, class-string<b>, true> */
$b = new b;
$b->d["testKey"] = 123;

// TODO
//$b->c["testValue"] = 123;
//$b->e["b"] = 123;
                    
