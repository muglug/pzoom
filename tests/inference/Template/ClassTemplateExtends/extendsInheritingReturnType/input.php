<?php
/**
 * @template T1
 */
class Container {
    /**
     * @return T1
     * @psalm-suppress InvalidReturnStatement
     * @psalm-suppress InvalidReturnType
     */
    public function get(int $key) { return new stdClass(); }
}

/**
 * @template T2
 * @inherits Container<T2>
 */
class ContainerSubclass extends Container {
    /**
     * @psalm-suppress InvalidReturnType
     */
    public function get(int $key) {}
}

class MyTestContainerUsage {
    /** @var ContainerSubclass<stdClass> */
    private $container;

    /** @param ContainerSubclass<stdClass> $container */
    public function __construct(ContainerSubclass $container) {
        $this->container = $container;
    }

    public function modify() : void {
        $this->container->get(1)->foo = 2;
    }
}