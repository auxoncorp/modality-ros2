#ifndef _GNU_SOURCE
#define _GNU_SOURCE
#endif

#include <dlfcn.h>
#include <stdio.h>
#include <time.h>

#include "modality_ros2.h" /* Only used to catch general syntax errors */

/***************************************************************************/
/* Hook functions that are called after the underlying rmw invocation.The  */
/* passthrough invocations happen in C so we don't have Rust frames on the */
/* stack at the same time; sometimes the rmw layer throws C++ exceptions,  */
/* and that's bad mojo (undefined behavior) in Rust.                       */
/***************************************************************************/

void modality_after_rmw_create_node(char *name, char *namespace,
                                    void *node_ptr);

void modality_after_rmw_destroy_node(void *node_ptr);

void modality_after_rmw_create_publisher(void *node_ptr, void *publisher_ptr,
                                         void *type_support, char *topic_name);

void modality_after_rmw_destroy_publisher(void *publisher_ptr);

void modality_before_rmw_publish(void *publisher_ptr, void *message);

void modality_after_clock_gettime(clockid_t clockid, struct timespec *tp);

void modality_after_rmw_create_subscription(void *node_ptr, void *sub_ptr,
                                            void *type_support,
                                            char *topic_name);

void modality_after_rmw_destroy_subscription(void *sub_ptr);

void modality_after_rmw_take_with_info(void *sub_ptr, void *message,
                                       void *message_info);

/* These are part of the modality-ros2 C API, we delegate to our implementation
 * and never call the user's implementation */
void modality_ros2_impl_set_next_publisher_nonce(void *rmw_publisher, uint64_t nonce);
void modality_ros2_impl_clear_next_publisher_nonce(void *rmw_publisher);
void modality_ros2_impl_set_next_subscription_nonce(void *rmw_subscription, uint64_t nonce);
void modality_ros2_impl_get_next_subscription_nonce(void *rmw_subscription, uint64_t *nonce);
void modality_ros2_impl_clear_next_subscription_nonce(void *rmw_subscription);

/***************************************/
/* Prototypes for the hooked functions */
/***************************************/

typedef void *(*rmw_create_node_t)(void *context, char *name, char *namespace);

typedef int (*rmw_destroy_node_t)(void *node_ptr);

typedef void *(*rmw_create_publisher_t)(void *node_ptr, void *type_support,
                                        char *topic_name, void *qos_policies,
                                        void *publisher_options);

typedef int (*rmw_destroy_publisher_t)(void *node_ptr, void *publisher_ptr);

typedef int (*rmw_publish_t)(void *pub_ptr, void *message, void *allocation);

typedef int (*clock_gettime_t)(clockid_t clockid, struct timespec *tp);

typedef void *(*rmw_create_subscription_t)(void *node_ptr, void *type_support,
                                           char *topic_name, void *qos_policies,
                                           void *subscription_options);

typedef int (*rmw_destroy_subscription_t)(void *node_ptr, void *sub_ptr);

typedef int (*rmw_take_with_info_t)(void *sub_ptr, void *message, void *taken,
                                    void *message_info, void *allocation);

/************************************************************/
/* Pointers to underlying versions of the hooked functions, */
/* initialized on demand.                                   */
/************************************************************/

static rmw_create_node_t real_rmw_create_node;
static rmw_destroy_node_t real_rmw_destroy_node;
static rmw_create_publisher_t real_rmw_create_publisher;
static rmw_destroy_publisher_t real_rmw_destroy_publisher;
static rmw_publish_t real_rmw_publish;
static clock_gettime_t real_clock_gettime;
static rmw_create_subscription_t real_rmw_create_subscription;
static rmw_destroy_subscription_t real_rmw_destroy_subscription;
static rmw_take_with_info_t real_rmw_take_with_info;

/***********************/
/* Hook function impls */
/***********************/

void *rmw_create_node(void *context, char *name, char *namespace) {
  if (!real_rmw_create_node) {
    real_rmw_create_node = dlsym(RTLD_NEXT, "rmw_create_node");
  }

  void *node_ptr = real_rmw_create_node(context, name, namespace);
  modality_after_rmw_create_node(name, namespace, node_ptr);

  return node_ptr;
}

int rmw_destroy_node_node(void *node_ptr) {
  if (!real_rmw_destroy_node) {
    real_rmw_destroy_node = dlsym(RTLD_NEXT, "rmw_destroy_node");
  }

  int ret = real_rmw_destroy_node(node_ptr);
  modality_after_rmw_destroy_node(node_ptr);

  return ret;
}

void *rmw_create_publisher(void *node_ptr, void *type_support, char *topic_name,
                           void *qos_policies, void *publisher_options) {
  if (!real_rmw_create_publisher) {
    real_rmw_create_publisher = dlsym(RTLD_NEXT, "rmw_create_publisher");
  }

  void *pub_ptr = real_rmw_create_publisher(node_ptr, type_support, topic_name,
                                            qos_policies, publisher_options);

  modality_after_rmw_create_publisher(node_ptr, pub_ptr, type_support,
                                      topic_name);
  return pub_ptr;
}

int rmw_destroy_publisher(void *node_ptr, void *pub_ptr) {
  if (!real_rmw_destroy_publisher) {
    real_rmw_destroy_publisher = dlsym(RTLD_NEXT, "rmw_destroy_publisher");
  }

  int ret = real_rmw_destroy_publisher(node_ptr, pub_ptr);
  modality_after_rmw_destroy_publisher(pub_ptr);

  return ret;
}

int rmw_publish(void *pub_ptr, void *message, void *allocation) {
  if (!real_rmw_publish) {
    real_rmw_publish = dlsym(RTLD_NEXT, "rmw_publish");
  }

  modality_before_rmw_publish(pub_ptr, message);
  return real_rmw_publish(pub_ptr, message, allocation);
}

int clock_gettime(clockid_t clockid, struct timespec *tp) {
  if (!real_clock_gettime) {
    real_clock_gettime = dlsym(RTLD_NEXT, "clock_gettime");
  }

  int ret = real_clock_gettime(clockid, tp);

  // 0 means success
  if (ret == 0) {
    modality_after_clock_gettime(clockid, tp);
  }

  return ret;
}

void *rmw_create_subscription(void *node_ptr, void *type_support,
                              char *topic_name, void *qos_policies,
                              void *subscription_options) {
  if (!real_rmw_create_subscription) {
    real_rmw_create_subscription = dlsym(RTLD_NEXT, "rmw_create_subscription");
  }

  void *sub_ptr = real_rmw_create_subscription(
      node_ptr, type_support, topic_name, qos_policies, subscription_options);
  modality_after_rmw_create_subscription(node_ptr, sub_ptr, type_support,
                                         topic_name);

  return sub_ptr;
}

int rmw_destroy_subscription(void *node_ptr, void *sub_ptr) {
  if (!real_rmw_destroy_subscription) {
    real_rmw_destroy_subscription =
        dlsym(RTLD_NEXT, "rmw_destroy_subscription");
  }

  int ret = real_rmw_destroy_subscription(node_ptr, sub_ptr);
  modality_after_rmw_destroy_subscription(sub_ptr);

  return ret;
}

int rmw_take_with_info(void *sub_ptr, void *message, void *taken,
                       void *message_info, void *allocation) {
  if (!real_rmw_take_with_info) {
    real_rmw_take_with_info = dlsym(RTLD_NEXT, "rmw_take_with_info");
  }

  int ret = real_rmw_take_with_info(sub_ptr, message, taken, message_info,
                                    allocation);

  // 0 is success
  if (ret == 0) {
    modality_after_rmw_take_with_info(sub_ptr, message, message_info);
  }

  return ret;
}

void modality_ros2_set_next_publisher_nonce(void *rmw_publisher, uint64_t nonce) {
    modality_ros2_impl_set_next_publisher_nonce(rmw_publisher, nonce);
}

void modality_ros2_clear_next_publisher_nonce(void *rmw_publisher) {
    modality_ros2_impl_clear_next_publisher_nonce(rmw_publisher);
}

void modality_ros2_set_next_subscription_nonce(void *rmw_subscription, uint64_t nonce) {
    modality_ros2_impl_set_next_subscription_nonce(rmw_subscription, nonce);
}

void modality_ros2_get_next_subscription_nonce(void *rmw_subscription, uint64_t *nonce) {
    modality_ros2_impl_get_next_subscription_nonce(rmw_subscription, nonce);
}

void modality_ros2_clear_next_subscription_nonce(void *rmw_subscription) {
    modality_ros2_impl_clear_next_subscription_nonce(rmw_subscription);
}

