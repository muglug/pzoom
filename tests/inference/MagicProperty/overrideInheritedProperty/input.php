<?php
interface ServiceInterface {}

class ConcreteService implements ServiceInterface {
    public function getById(int $i) : void {}
}

class Foo
{
    /** @var ServiceInterface */
    protected $service;

    public function __construct(ServiceInterface $service)
    {
        $this->service = $service;
    }
}

/** @property ConcreteService $service */
class FooBar extends Foo
{
    public function __construct(ConcreteService $concreteService)
    {
        parent::__construct($concreteService);
    }

    public function doSomething(): void
    {
        $this->service->getById(123);
    }
}
