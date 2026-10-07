<?php
/**
 * Plugin Name: Byakko Keyboard Tester
 * Description: A lightweight keyboard input tester for WordPress pages.
 * Version: 0.1.0
 * License: GPL-3.0-or-later
 * License URI: https://www.gnu.org/licenses/gpl-3.0.html
 * Text Domain: byakko-keyboard-tester
 */

// SPDX-License-Identifier: GPL-3.0-or-later
if ( ! defined( 'ABSPATH' ) ) {
    exit;
}

/** Load the tester only on a page whose content contains its shortcode. */
function byakko_keyboard_tester_enqueue_for_page() {
    $post = get_queried_object();
    if ( ! $post instanceof WP_Post || ! has_shortcode( $post->post_content, 'byakko_keyboard_tester' ) ) {
        return;
    }

    wp_enqueue_style(
        'byakko-keyboard-tester',
        plugins_url( 'tester.css', __FILE__ ),
        array(),
        '0.1.0'
    );
    wp_enqueue_script(
        'byakko-keyboard-tester',
        plugins_url( 'tester.js', __FILE__ ),
        array(),
        '0.1.0',
        true
    );
}
add_action( 'wp_enqueue_scripts', 'byakko_keyboard_tester_enqueue_for_page' );

/** Return indexable HTML immediately; JavaScript adds the interactive tester. */
function byakko_keyboard_tester_shortcode() {
    $fragment = __DIR__ . '/tester.html';
    if ( ! is_readable( $fragment ) ) {
        return '<p>' . esc_html__( 'The keyboard tester is temporarily unavailable.', 'byakko-keyboard-tester' ) . '</p>';
    }

    $html = file_get_contents( $fragment );
    if ( false === $html ) {
        return '<p>' . esc_html__( 'The keyboard tester is temporarily unavailable.', 'byakko-keyboard-tester' ) . '</p>';
    }
    return $html;
}
add_shortcode( 'byakko_keyboard_tester', 'byakko_keyboard_tester_shortcode' );
