<?php
enum Transport: string {
    case CAR = 'car';
    case BIKE = 'bike';
    case BOAT = 'boat';
}

$val = Transport::from(uniqid());
$_name = $val->name;
