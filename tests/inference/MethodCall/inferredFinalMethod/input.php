<?php
class PropertyClass {
    public function test() : bool {
        return true;
    }
}

class MainClass {
    private ?PropertyClass $property = null;

    final public function getProperty(): ?PropertyClass {
        return $this->property;
    }
}

$main = new MainClass();

if ($main->getProperty() !== null && $main->getProperty()->test()) {}
