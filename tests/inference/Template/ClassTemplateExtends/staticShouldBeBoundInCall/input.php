<?php
/**
 * @template TVehicle of Vehicle
 */
class VehicleCollection {
    /**
     * @param class-string<TVehicle> $item
     */
    public function __construct(string $item) {}
}

abstract class Vehicle {
    /**
     * @return VehicleCollection<static>
     */
    public static function all() {
        return new VehicleCollection(static::class);
    }
}

class Car extends Vehicle {}

class CarRepository {
    /**
    * @return VehicleCollection<Car>
    */
    public function getAllCars(): VehicleCollection {
        return Car::all();
    }
}