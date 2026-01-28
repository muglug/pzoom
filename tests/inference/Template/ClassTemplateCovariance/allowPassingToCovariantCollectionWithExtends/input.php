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

/** @template-extends Collection<Dog> */
class HardwiredDogCollection extends Collection {}

/**
 * @param Collection<Animal> $list
 */
function getSounds(Collection $list) : void {
    foreach ($list as $l) {
        echo $l->getSound();
    }
}

getSounds(new HardwiredDogCollection([new Dog]));