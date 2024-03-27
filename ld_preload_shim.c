#ifndef _GNU_SOURCE
#define _GNU_SOURCE
#include "rmw/init.h"
#include "rmw/types.h"
#endif

#include <dlfcn.h>
#include <stdio.h>
#include <time.h>

#include "rmw/rmw.h"

/***************************************************************************/
/* Hook functions that are called after the underlying rmw invocation.The  */
/* passthrough invocations happen in C so we don't have Rust frames on the */
/* stack at the same time; sometimes the rmw layer throws C++ exceptions,  */
/* and that's bad mojo (undefined behavior) in Rust.                       */
/***************************************************************************/

void modality_after_rmw_create_node(const char *name, const char *namespace, rmw_node_t *node_ptr);
void modality_after_rmw_destroy_node(const uintptr_t destroyed_node_addr);
void modality_after_rmw_create_publisher(const rmw_node_t *node, const rmw_publisher_t *publisher, const rosidl_message_type_support_t *type_support, const char *topic_name);
void modality_after_rmw_destroy_publisher(const uintptr_t destroyed_publisher_addr);
void modality_before_rmw_publish(const rmw_publisher_t *publisher_ptr, const void *message);
void modality_after_clock_gettime(clockid_t clockid, struct timespec *tp);
void modality_after_rmw_create_subscription(const rmw_node_t *node, const rmw_subscription_t *subscription, const rosidl_message_type_support_t *type_support, const char *topic_name);
void modality_after_rmw_destroy_subscription(const uintptr_t destroyed_subscription_addr);
void modality_after_rmw_take_with_info(const rmw_subscription_t *sub_ptr, void *ros_message, rmw_message_info_t *message_info);

/***************************************/
/* Prototypes for the hooked functions */
/***************************************/

typedef rmw_node_t *(*rmw_create_node_t)(rmw_context_t *rmw_context_t, const char *name, const char *namespace_);
typedef rmw_ret_t (*rmw_destroy_node_t)(rmw_node_t *node);
typedef rmw_publisher_t *(*rmw_create_publisher_t)(const rmw_node_t *node, const rosidl_message_type_support_t *type_support, const char *topic_name, const rmw_qos_profile_t *qos_profile, const rmw_publisher_options_t *publisher_options);
typedef rmw_ret_t (*rmw_destroy_publisher_t)(rmw_node_t *node, rmw_publisher_t *publisher_ptr);
typedef rmw_ret_t (*rmw_publish_t)(const rmw_publisher_t *pub_ptr, const void *message, rmw_publisher_allocation_t *allocation);
typedef rmw_ret_t (*clock_gettime_t)(clockid_t clockid, struct timespec *tp);
typedef rmw_subscription_t *(*rmw_create_subscription_t)(const rmw_node_t *node_ptr, const rosidl_message_type_support_t *type_support, const char *topic_name, const rmw_qos_profile_t *qos_policies, const rmw_subscription_options_t *subscription_options);
typedef rmw_ret_t (*rmw_destroy_subscription_t)(const rmw_node_t *node_ptr, rmw_subscription_t *sub_ptr);
typedef rmw_ret_t (*rmw_take_with_info_t)(const rmw_subscription_t * subscription, void *ros_message, bool *taken, rmw_message_info_t * message_info, rmw_subscription_allocation_t * allocation);

/************************************************************/
/* Pointers to underlying versions of the hooked functions, */
/* initialized on demand.                                   */
/************************************************************/

rmw_create_node_t real_rmw_create_node;
rmw_destroy_node_t real_rmw_destroy_node;
rmw_create_publisher_t real_rmw_create_publisher;
rmw_destroy_publisher_t real_rmw_destroy_publisher;
rmw_publish_t real_rmw_publish;
clock_gettime_t real_clock_gettime;
rmw_create_subscription_t real_rmw_create_subscription;
rmw_destroy_subscription_t real_rmw_destroy_subscription;
rmw_take_with_info_t real_rmw_take_with_info;

/***********************/
/* Hook function impls */
/***********************/

rmw_node_t *rmw_create_node(
  rmw_context_t *context,
  const char *name,
  const char *namespace
) {
  if (!real_rmw_create_node) {
    real_rmw_create_node = dlsym(RTLD_NEXT, "rmw_create_node");
  }

  void *node_ptr = real_rmw_create_node(context, name, namespace);
  modality_after_rmw_create_node(name, namespace, node_ptr);

  return node_ptr;
}

rmw_ret_t rmw_destroy_node(rmw_node_t *node_ptr) {
  if (!real_rmw_destroy_node) {
    real_rmw_destroy_node = dlsym(RTLD_NEXT, "rmw_destroy_node");
  }

  int ret = real_rmw_destroy_node(node_ptr);
  modality_after_rmw_destroy_node((uintptr_t)node_ptr);

  return ret;
}

rmw_publisher_t *rmw_create_publisher(
  const rmw_node_t *node,
  const rosidl_message_type_support_t *type_support,
  const char *topic_name,
  const rmw_qos_profile_t *qos_profile,
  const rmw_publisher_options_t *publisher_options
) {
  if (!real_rmw_create_publisher) {
    real_rmw_create_publisher = dlsym(RTLD_NEXT, "rmw_create_publisher");
  }

  void *pub_ptr = real_rmw_create_publisher(node, type_support, topic_name, qos_profile, publisher_options);
  modality_after_rmw_create_publisher(node, pub_ptr, type_support, topic_name);

  return pub_ptr;
}

rmw_ret_t rmw_destroy_publisher(rmw_node_t *node_ptr, rmw_publisher_t *pub_ptr) {
  if (!real_rmw_destroy_publisher) {
    real_rmw_destroy_publisher = dlsym(RTLD_NEXT, "rmw_destroy_publisher");
  }

  int ret = real_rmw_destroy_publisher(node_ptr, pub_ptr);
  modality_after_rmw_destroy_publisher((uintptr_t)pub_ptr);

  return ret;
}

rmw_ret_t rmw_publish(
  const rmw_publisher_t * publisher,
  const void * ros_message,
  rmw_publisher_allocation_t * allocation
) {
  if (!real_rmw_publish) {
    real_rmw_publish = dlsym(RTLD_NEXT, "rmw_publish");
  }

  modality_before_rmw_publish(publisher, ros_message);
  return real_rmw_publish(publisher, ros_message, allocation);
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

rmw_subscription_t *rmw_create_subscription(
  const rmw_node_t * node,
  const rosidl_message_type_support_t * type_support,
  const char * topic_name,
  const rmw_qos_profile_t * qos_policies,
  const rmw_subscription_options_t * subscription_options
) {
  if (!real_rmw_create_subscription) {
    real_rmw_create_subscription = dlsym(RTLD_NEXT, "rmw_create_subscription");
  }

  void *sub_ptr = real_rmw_create_subscription(node, type_support, topic_name, qos_policies, subscription_options);
  modality_after_rmw_create_subscription(node, sub_ptr, type_support, topic_name);

  return sub_ptr;
}

rmw_ret_t rmw_destroy_subscription(
  rmw_node_t * node,
  rmw_subscription_t * subscription
) {
  if (!real_rmw_destroy_subscription) {
    real_rmw_destroy_subscription = dlsym(RTLD_NEXT, "rmw_destroy_subscription");
  }

  int ret = real_rmw_destroy_subscription(node, subscription);
  modality_after_rmw_destroy_subscription((uintptr_t)subscription);

  return ret;
}

rmw_ret_t rmw_take_with_info(
  const rmw_subscription_t * subscription,
  void * ros_message,
  bool * taken,
  rmw_message_info_t * message_info,
  rmw_subscription_allocation_t * allocation
) {
  if (!real_rmw_take_with_info) {
    real_rmw_take_with_info = dlsym(RTLD_NEXT, "rmw_take_with_info");
  }

  int ret = real_rmw_take_with_info(subscription, ros_message, taken, message_info, allocation);
  if (ret == RMW_RET_OK) {
    modality_after_rmw_take_with_info(subscription, ros_message, message_info);
  }

  return ret;
}
