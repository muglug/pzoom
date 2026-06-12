<?php
enum Status: string
{
    case DRAFT = "draft";
    case PUBLISHED = "published";
    case ARCHIVED = "archived";

    public function get(): string
    {
        return $this->value;
    }
}

echo Status::DRAFT->get();
