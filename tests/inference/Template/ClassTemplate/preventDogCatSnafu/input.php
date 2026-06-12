<?php
class Animal {}
class Dog extends Animal {}
class Cat extends Animal {}

/**
 * @template T
 */
class Collection {
    /**
     * @param T $t
     */
    public function add($t) : void {}
}

/**
 * @param Collection<Animal> $list
 */
function addAnimal(Collection $list) : void {
    $list->add(new Cat());
}

/**
 * @param Collection<Dog> $list
 */
function takesDogList(Collection $list) : void {
    addAnimal($list); // this should be an error
}
