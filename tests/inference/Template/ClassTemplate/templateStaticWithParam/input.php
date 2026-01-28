<?php
/**
 * @template T
 * @psalm-consistent-constructor
 * @psalm-consistent-templates
 */
class ArrayCollection {
    /** @var list<T> */
    private $elements;

    /**
     * @param list<T> $elements
     */
    public function __construct(array $elements) {
        $this->elements = $elements;
    }

    /**
     * @template U
     * @param callable(T):U $callback
     * @return static<U>
     */
    public function map(callable $callback) {
        /** @psalm-suppress RedundantFunctionCall */
        return new static(array_values(array_map($callback, $this->elements)));
    }
}

/** @param ArrayCollection<int<0, max>> $ints */
function takesInts(ArrayCollection $ints) :void {}

/** @param ArrayCollection<int|string> $ints */
function takesIntsOrStrings(ArrayCollection $ints) :void {}

/** @return list<string> */
function getList() :array {return [];}

takesInts((new ArrayCollection(getList()))->map("strlen"));

/** @return ($s is "string" ? string : int) */
function foo(string $s) {
    if ($s === "string") {
        return "hello";
    }
    return 5;
}

takesIntsOrStrings((new ArrayCollection(getList()))->map("foo"));

/**
 * @template T
 * @extends ArrayCollection<T>
 */
class LazyArrayCollection extends ArrayCollection {}
