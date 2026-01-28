<?php
class PropertyClass {
    public function test() : void {
        echo "test";
    }
}
class SomeClass {
    private ?PropertyClass $property = null;

    private function getProperty(): ?PropertyClass {
        return $this->property;
    }

    public function test(int $int): void {
        if ($this->getProperty() !== null) {
            $this->getProperty()->test();
        }
    }
}
