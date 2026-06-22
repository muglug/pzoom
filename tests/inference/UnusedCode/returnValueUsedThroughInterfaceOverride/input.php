<?php

// Calling a concrete override and using its return value also counts as using
// the return value of the interface/abstract method it implements, so the
// interface declaration is NOT reported as PossiblyUnusedReturnValue.

interface Named
{
    public function getName(): string;
}

final class Widget implements Named
{
    public function getName(): string
    {
        return 'w';
    }
}

final class Printer
{
    public function __construct(private Widget $w) {}

    public function run(): string
    {
        return $this->w->getName();
    }
}

$p = new Printer(new Widget());
echo $p->run();
