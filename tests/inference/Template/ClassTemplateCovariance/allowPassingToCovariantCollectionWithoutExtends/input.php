<?php
abstract class Animal {
    abstract public function getSound() : string;
}
class Dog extends Animal {
    public function getSound() : string {
        return "Woof!";
    }
}

/**
 * @template-covariant TValue
 * @implements IteratorAggregate<int, TValue>
 */
class Collection implements IteratorAggregate {
    private $arr;

    /**
     * @param array<int,TValue> $arr
     */
    public function __construct(array $arr) {
        $this->arr = $arr;
    }

    /** @return Traversable<int, TValue> */
    public function getIterator() {
        foreach ($this->arr as $k => $v) {
            yield $k => $v;
        }
    }
}

/**
 * @param Collection<Animal> $list
 */
function getSounds(Collection $list) : void {
    foreach ($list as $l) {
        $l->getSound();
    }
}

/**
 * @param Collection<Dog> $list
 */
function takesDogList(Collection $list) : void {
    getSounds($list); // this probably should not be an error
}