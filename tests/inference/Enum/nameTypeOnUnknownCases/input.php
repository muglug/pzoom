<?php
enum Transport: string {
    case CAR = 'car';
    case BIKE = 'bike';
    case BOAT = 'boat';
}

function f(Transport $e): void {
    $_name = $e->name;
    /** @psalm-check-type-exact $_name='BIKE'|'BOAT'|'CAR' */;
}
