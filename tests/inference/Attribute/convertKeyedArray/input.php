<?php
#[Attribute(Attribute::TARGET_CLASS)]
class Route {
    private $methods = [];
    /**
     * @param string[] $methods
     */
    public function __construct(array $methods = []) {
        $this->methods = $methods;
    }
}
#[Route(methods: ["GET"])]
class HealthController
{}
