<?php
enum Status
{
    case DRAFT;
    case PUBLISHED;
    case ARCHIVED;
}
$a = Status::DRAFT->name;
