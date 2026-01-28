<?php
class Animal {}

/**
 * @template DC1 of Animal
 */
interface IStroker {
    /**
     * @psalm-param DC1 $animal
     */
    public function stroke(Animal $animal): void;
}

/**
 * @template DC2 of Animal
 * @template-extends IStroker<DC2>
 */
interface IStroker2 extends IStroker {}

class Dog extends Animal {}

/**
 * @implements IStroker2<Dog>
 */
class DogStroker implements IStroker2 {
    public function stroke(Animal $animal): void {
        $this->doDeletePerson($animal);
    }

    private function doDeletePerson(Dog $animal): void {}
}