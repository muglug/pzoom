<?php
interface MainInterface {
    public const TEST = 'test';
}

interface FooInterface extends MainInterface {}
interface BarInterface extends MainInterface {}

class FooBar implements FooInterface, BarInterface {}
