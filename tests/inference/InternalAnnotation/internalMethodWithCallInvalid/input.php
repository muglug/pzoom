<?php
                    namespace A {
                        class Foo {
                            /**
                             * @internal
                             */
                            public static function barBar(): void {
                            }
                        }
                    }

                    namespace B {
                        class Bat {
                            public function batBat(): void {
                                \A\Foo::barBar();
                            }
                        }
                    }
