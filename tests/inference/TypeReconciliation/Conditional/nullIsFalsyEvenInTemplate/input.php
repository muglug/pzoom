<?php

abstract class Animal {
    public function foo(): void {}
}

class Dog extends Animal {}
class Cat extends Animal {}

/**
 * @template T
 */
interface RepositoryInterface
{
    /**
    * @return ?T
    */
    public function find(int $id);
}

/**
 * @template T
 * @implements RepositoryInterface<T>
 */
class AbstarctRepository  implements RepositoryInterface
{
    public function find(int $id) {
        return null;
    }
}

/**
 * @template T as Animal
 * @extends AbstarctRepository<T>
 */
class AnimalRepository extends AbstarctRepository {}

/**
 * @extends AnimalRepository<Cat>
 */
class CatRepository extends AnimalRepository {}

/**
 * @extends AnimalRepository<Dog>
 */
class DogRepository extends AnimalRepository {}

function doSomething(AnimalRepository $repository) : void {
    $foo = $repository->find(1);

    if (!$foo) {
        return;
    }

    $foo->foo();
}