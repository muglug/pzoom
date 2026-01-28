<?php
class SomeService
{
    /**
     * @var \SplObjectStorage<\stdClass, mixed>
     */
    public $handlers;

    /**
     * @param SplObjectStorage<\stdClass, mixed> $handlers
     */
    public function __construct(SplObjectStorage $handlers)
    {
        $this->handlers = $handlers;
    }
}

/** @var SplObjectStorage<\stdClass, mixed> */
$storage = new SplObjectStorage();
new SomeService($storage);

$c = new \stdClass();
$storage[$c] = "hello";
/** @psalm-suppress MixedAssignment */
$b = $storage->offsetGet($c);
