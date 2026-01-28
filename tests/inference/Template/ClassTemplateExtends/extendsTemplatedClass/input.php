<?php
class Animal {}

/**
 * @template DC1 of Animal
 */
class IStroker {
    /**
     * @psalm-param DC1 $animal
     */
    public function stroke(Animal $animal): void {}
}

/**
 * @template DC2 of Animal
 * @template-extends IStroker<DC2>
 */
class IStroker2 extends IStroker {}

class Dog extends Animal {}

/**
 * @extends IStroker2<Dog>
 */
class DogStroker extends IStroker2 {
    public function stroke(Animal $animal): void {
        $this->doDeletePerson($animal);
    }

    private function doDeletePerson(Dog $animal): void {}
}