<?php

final class Cat {
    public function speak(): string { return 'meow'; }
}

final class Dog {
    public function speak(): string { return 'woof'; }
}

final class Speaker {
    public function announce(Cat|Dog $animal): string {
        return $animal->speak();
    }
}

$speaker = new Speaker();
echo $speaker->announce(new Cat());
echo $speaker->announce(new Dog());
